use std::collections::HashMap;
use std::fmt::Write;
use std::fs::File;
use std::io::Read;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::{Response, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use shuttle_common::project::ProjectName;
use shuttle_common::{ApiKey, ApiUrl, DeploymentMeta, DeploymentStateMeta, SHUTTLE_PROJECT_HEADER};
use tokio::time::sleep;

use crate::print;

pub(crate) async fn auth(mut api_url: ApiUrl, username: String) -> Result<ApiKey> {
    let client = get_retry_client();

    let _ = write!(api_url, "/users/{}", username);

    let res: Response = client
        .post(api_url)
        .send()
        .await
        .context("failed to get API key from Shuttle server")?;

    let response_status = res.status();
    let response_text = res.text().await?;

    if response_status == StatusCode::OK {
        return Ok(response_text);
    }

    Err(anyhow!(
        "status: {}, body: {}",
        response_status,
        response_text
    ))
}

pub(crate) async fn delete(
    mut api_url: ApiUrl,
    api_key: &ApiKey,
    project: &ProjectName,
) -> Result<()> {
    let client = get_retry_client();

    let _ = write!(api_url, "/projects/{}", project);
    let res: Response = client
        .delete(api_url)
        .basic_auth(api_key, Some(""))
        .send()
        .await
        .context("failed to delete deployment on the Shuttle server")?;

    let deployment_meta = to_api_result(res).await?;

    println!("{}", deployment_meta);

    Ok(())
}

pub(crate) async fn status(api_url: ApiUrl, api_key: &ApiKey, project: &ProjectName) -> Result<()> {
    let client = get_retry_client();

    let deployment_meta = get_deployment_meta(api_url, api_key, project, &client).await?;

    println!("{}", deployment_meta);

    Ok(())
}

pub(crate) async fn shuttle_version(mut api_url: ApiUrl) -> Result<String> {
    let client = get_retry_client();
    api_url.push_str("/version");

    let res: Response = client
        .get(api_url)
        .send()
        .await
        .context("failed to get version from Shuttle server")?;

    let response_status = res.status();

    if response_status == StatusCode::OK {
        Ok(res.text().await?)
    } else {
        Err(anyhow!(
            "status: {}, body: {}",
            response_status,
            res.text().await?
        ))
    }
}

pub(crate) async fn logs(api_url: ApiUrl, api_key: &ApiKey, project: &ProjectName) -> Result<()> {
    let client = get_retry_client();

    let deployment_meta = get_deployment_meta(api_url, api_key, project, &client).await?;

    for (datetime, log_item) in deployment_meta.runtime_logs {
        print::log(datetime, log_item);
    }

    Ok(())
}

async fn get_deployment_meta(
    mut api_url: ApiUrl,
    api_key: &ApiKey,
    project: &ProjectName,
    client: &ClientWithMiddleware,
) -> Result<DeploymentMeta> {
    let _ = write!(api_url, "/projects/{}", project);

    let res: Response = client
        .get(api_url)
        .basic_auth(api_key.clone(), Some(""))
        .send()
        .await
        .context("failed to get deployment metadata")?;

    to_api_result(res).await
}

fn get_retry_client() -> ClientWithMiddleware {
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    ClientBuilder::new(reqwest::Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}

pub(crate) async fn deploy(
    package_file: File,
    api_url: ApiUrl,
    api_key: &ApiKey,
    project: &ProjectName,
) -> Result<DeploymentStateMeta> {
    let mut url = api_url.clone();
    let _ = write!(url, "/projects/{}", project.as_str());

    //panic!("{}", url);
    //http://localhost:8002/projects/hello-world-axum-app

    let client = get_retry_client();

    let mut package_file = package_file;
    let mut package_content = Vec::new();
    package_file
        .read_to_end(&mut package_content)
        .context("failed to convert package content to buf")?;

    let res: Response = client
        .post(url)
        .body(package_content)
        .header(SHUTTLE_PROJECT_HEADER, serde_json::to_string(&project)?)
        .basic_auth(api_key.clone(), Some(""))
        .send()
        .await
        .context("failed to send deployment to the Shuttle server")?;

    let mut deployment_meta = to_api_result(res).await?;

    let mut log_pos = 0;

    while !matches!(
        deployment_meta.state,
        DeploymentStateMeta::Deployed | DeploymentStateMeta::Error(_)
    ) {
        print_log(&deployment_meta.build_logs, &mut log_pos);

        sleep(Duration::from_millis(350)).await;

        deployment_meta = get_deployment_meta(api_url.clone(), api_key, project, &client).await?;
    }

    print_log(&deployment_meta.build_logs, &mut log_pos);

    println!("{}", &deployment_meta);

    Ok(deployment_meta.state)
}

pub(crate) async fn secrets(
    mut api_url: ApiUrl,
    api_key: &ApiKey,
    project: &ProjectName,
    secrets: HashMap<String, String>,
) -> Result<()> {
    if secrets.is_empty() {
        return Ok(());
    }

    let _ = write!(api_url, "/projects/{}/secrets/", project.as_str());

    let client = get_retry_client();

    client
        .post(api_url)
        .body(serde_json::to_string(&secrets)?)
        .header(SHUTTLE_PROJECT_HEADER, serde_json::to_string(&project)?)
        .basic_auth(api_key.clone(), Some(""))
        .send()
        .await
        .context("failed to send deployment's secrets to the Shuttle server")
        .map(|_| ())
}

fn print_log(logs: &Option<String>, log_pos: &mut usize) {
    if let Some(logs) = logs {
        let new = &logs[*log_pos..];

        if !new.is_empty() {
            *log_pos = logs.len();
            print!("{}", new);
        }
    }
}

async fn to_api_result(res: Response) -> Result<DeploymentMeta> {
    let text = res.text().await?;
    match serde_json::from_str::<DeploymentMeta>(&text) {
        Ok(meta) => Ok(meta),
        Err(_) => Err(anyhow!("{}", text)),
    }
}
