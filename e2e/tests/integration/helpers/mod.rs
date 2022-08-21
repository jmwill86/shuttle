use std::io::{self, stderr, stdout, BufRead, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::str;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use std::{env, path};

use colored::*;
use reqwest::blocking::RequestBuilder;

use lazy_static::lazy_static;
use tempdir::TempDir;

/// The directory given to `cargo shuttle` run in the context of E2E
/// testing
pub enum TempCargoHome {
    /// A directory managed by the caller, no patch applied
    User(PathBuf),
    /// A directory managed by this crate is created, applies the
    /// patch as required
    Managed(TempDir),
}

impl TempCargoHome {
    /// Initialize a new `TempCargoHome` with a `shuttle-service`
    /// patch unless `SHUTTLE_CARGO_HOME` is set, then use that. With
    /// the latter, no patch is applied
    pub fn from_env_or_new() -> Self {
        match env::var("SHUTTLE_CARGO_HOME") {
            Ok(path) => Self::User(path.into()),
            Err(_) => {
                let dir = TempDir::new("shuttle-tests").unwrap();

                // Apply the `patch.crates-io` for `shuttle-service`
                let mut config = std::fs::File::create(dir.path().join("config.toml")).unwrap();
                write!(
                    config,
                    r#"[patch.crates-io]
shuttle-service = {{ path = "{}" }}"#,
                    WORKSPACE_ROOT.join("service").display()
                )
                .unwrap();

                Self::Managed(dir)
            }
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::User(path) => path.as_path(),
            Self::Managed(dir) => dir.path()
        }
    }

    pub fn display(&self) -> path::Display<'_> {
        self.path().display()
    }
}

lazy_static! {
    static ref WORKSPACE_ROOT: PathBuf = {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    };
    static ref DOCKER: PathBuf = which::which("docker").unwrap();
    static ref MAKE: PathBuf = which::which("make").unwrap();
    static ref CARGO: PathBuf = which::which("cargo").unwrap();
    static ref DB_FQDN: String = env::var("DB_FQDN").unwrap();
    pub static ref APPS_FQDN: String = env::var("APPS_FQDN").unwrap();
    static ref CARGO_HOME: TempCargoHome = TempCargoHome::from_env_or_new();
    static ref LOCAL_UP: () = {
        println!(
            "
----------------------------------- PREPARING ------------------------------------
docker: {}
make: {}
cargo: {}
CARGO_HOME: {}
----------------------------------------------------------------------------------
",
            DOCKER.display(),
            MAKE.display(),
            CARGO.display(),
            CARGO_HOME.display()
        );

        Command::new(MAKE.as_os_str())
            .arg("up")
            .current_dir(WORKSPACE_ROOT.as_path())
            .output()
            .ensure_success("failed to `make up`");

        Command::new(CARGO.as_os_str())
            .args(["build", "--bin", "cargo-shuttle"])
            .current_dir(WORKSPACE_ROOT.as_path())
            .output()
            .ensure_success("failed to `cargo build --bin cargo-shuttle`");
    };
}

trait EnsureSuccess {
    fn ensure_success<S: AsRef<str>>(self, s: S);
}

impl EnsureSuccess for io::Result<ExitStatus> {
    fn ensure_success<S: AsRef<str>>(self, s: S) {
        let exit_status = self.unwrap();
        if !exit_status.success() {
            panic!("{}: exit code {}", s.as_ref(), exit_status)
        }
    }
}

impl EnsureSuccess for io::Result<Output> {
    fn ensure_success<S: AsRef<str>>(self, s: S) {
        self.map(|output| {
            let _ = stderr().write_all(&output.stderr);
            let _ = stdout().write_all(&output.stdout);
            output.status
        })
        .ensure_success(s)
    }
}

pub fn log_lines<R: io::Read, D: std::fmt::Display>(mut reader: R, target: D) {
    let mut buf = [0; 2 << 17]; // 128kb
    let mut current_pos = 0;
    loop {
        let n = reader.read(&mut buf[current_pos..]).unwrap();
        if n == 0 {
            break;
        }
        current_pos += n;

        if buf[current_pos - 1] != b'\n' {
            continue;
        }

        for line in io::BufReader::new(&buf[..current_pos]).lines() {
            println!("{} {}", target, line.unwrap());
        }

        current_pos = 0;
    }

    // Log last
    if current_pos != 0 {
        for line in io::BufReader::new(&buf[..current_pos]).lines() {
            println!("{} {}", target, line.unwrap());
        }
    }
}

pub fn spawn_and_log<D: std::fmt::Display, C: Into<Color>>(
    cmd: &mut Command,
    target: D,
    color: C,
) -> Child {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let color = color.into();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdout_target = format!("{} >>>", target).color(color);
    let stderr_target = format!("{} >>>", target).bold().color(color);
    std::thread::spawn(move || log_lines(&mut stdout, stdout_target));
    std::thread::spawn(move || log_lines(&mut stderr, stderr_target));
    child
}

pub struct Services {
    api_addr: SocketAddr,
    proxy_addr: SocketAddr,
    target: String,
    color: Color,
}

impl Services {
    fn new_free<D, C>(target: D, color: C) -> Self
    where
        D: std::fmt::Display,
        C: Into<Color>,
    {
        Self {
            api_addr: "127.0.0.1:8001".parse().unwrap(),
            proxy_addr: "127.0.0.1:8000".parse().unwrap(),
            target: target.to_string(),
            color: color.into(),
        }
    }

    pub fn new_docker<D, C>(target: D, color: C) -> Self
    where
        D: std::fmt::Display,
        C: Into<Color>,
    {
        let _ = *LOCAL_UP;
        let service = Self::new_free(target, color);
        service.wait_ready(Duration::from_secs(15));
        service
    }

    pub fn wait_ready(&self, mut timeout: Duration) {
        let mut now = SystemTime::now();
        while !timeout.is_zero() {
            match reqwest::blocking::get(format!("http://{}/status", self.api_addr)) {
                Ok(resp) if resp.status().is_success() => return,
                _ => sleep(Duration::from_secs(1)),
            }
            timeout = timeout
                .checked_sub(now.elapsed().unwrap())
                .unwrap_or_default();
            now = SystemTime::now();
        }
        panic!("timed out while waiting for api to /status OK");
    }

    pub fn run_client<'s, I, P>(&self, args: I, path: P) -> Child
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = &'s str>,
    {
        let mut run = Command::new(WORKSPACE_ROOT.join("target/debug/cargo-shuttle"));

        if env::var("SHUTTLE_API_KEY").is_err() {
            run.env("SHUTTLE_API_KEY", "test-key");
        }

        run.env("CARGO_HOME", CARGO_HOME.path());

        run.args(args).current_dir(path);
        spawn_and_log(&mut run, &self.target, self.color)
    }

    pub fn deploy(&self, project_path: &str) {
        self.run_client(
            ["deploy", "--allow-dirty"],
            WORKSPACE_ROOT.join("examples").join(project_path),
        )
        .wait()
        .ensure_success("failed to run deploy");
    }

    pub fn get(&self, sub_path: &str) -> RequestBuilder {
        reqwest::blocking::Client::new().get(format!("http://{}/{}", self.proxy_addr, sub_path))
    }

    #[allow(dead_code)]
    pub fn post(&self, sub_path: &str) -> RequestBuilder {
        reqwest::blocking::Client::new().post(format!("http://{}/{}", self.proxy_addr, sub_path))
    }
}
