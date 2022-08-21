#[macro_use]
extern crate rocket;

#[macro_use]
extern crate log;

mod args;
mod auth;
mod auth_admin;
mod build;
mod deployment;
mod factory;
mod proxy;
mod router;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use auth_admin::Admin;
use clap::Parser;
pub use deployment::MAX_DEPLOYS;
use factory::ShuttleFactory;
use rocket::serde::json::Json;
use rocket::{tokio, Build, Data, Rocket, State};
use shuttle_common::project::ProjectName;
use shuttle_common::{DeploymentApiError, DeploymentMeta, Port};
use shuttle_service::SecretStore;
use uuid::Uuid;

use crate::args::Args;
use crate::auth::{ApiKey, AuthorizationError, ScopedUser, User, UserDirectory};
use crate::build::{BuildSystem, FsBuildSystem};
use crate::deployment::DeploymentSystem;

type ApiResult<T, E> = Result<Json<T>, E>;

/// Find user by username and return it's API Key.
/// if user does not exist create it and update `users` state to `users.toml`.
/// Finally return user's API Key.
#[post("/users/<username>")]
async fn get_or_create_user(
    user_directory: &State<UserDirectory>,
    username: String,
    _admin: Admin,
) -> Result<ApiKey, AuthorizationError> {
    user_directory.get_or_create(username)
}

/// Status API to be used to check if the service is alive
#[get("/status")]
async fn status() -> String {
    String::from("Ok")
}

#[get("/version")]
async fn version() -> String {
    String::from(shuttle_service::VERSION)
}

#[get("/<_>/deployments/<id>")]
async fn get_deployment(
    state: &State<ApiState>,
    id: Uuid,
    _user: ScopedUser,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    info!("[GET_DEPLOYMENT, {}, {}]", _user.name(), _user.scope());
    let deployment = state.deployment_manager.get_deployment(&id).await?;
    Ok(Json(deployment))
}

#[delete("/<_>/deployments/<id>")]
async fn delete_deployment(
    state: &State<ApiState>,
    id: Uuid,
    _user: ScopedUser,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    info!("[DELETE_DEPLOYMENT, {}, {}]", _user.name(), _user.scope());
    // TODO why twice?
    let _deployment = state.deployment_manager.get_deployment(&id).await?;
    let deployment = state.deployment_manager.kill_deployment(&id).await?;
    Ok(Json(deployment))
}

#[get("/<_>")]
async fn get_project(
    state: &State<ApiState>,
    user: ScopedUser,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    info!("[GET_PROJECT, {}, {}]", user.name(), user.scope());

    let deployment = state
        .deployment_manager
        .get_deployment_for_project(user.scope())
        .await?;

    Ok(Json(deployment))
}

#[delete("/<_>")]
async fn delete_project(
    state: &State<ApiState>,
    user: ScopedUser,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    info!("[DELETE_PROJECT, {}, {}]", user.name(), user.scope());

    let deployment = state
        .deployment_manager
        .kill_deployment_for_project(user.scope())
        .await?;
    Ok(Json(deployment))
}

#[post("/<project_name>", data = "<crate_file>")]
async fn create_project(
    state: &State<ApiState>,
    user_directory: &State<UserDirectory>,
    crate_file: Data<'_>,
    project_name: ProjectName,
    user: User,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    info!("[CREATE_PROJECT, {}, {}]", &user.name, &project_name);

    if !user
        .projects
        .iter()
        .any(|my_project| *my_project == project_name)
    {
        user_directory.create_project_if_not_exists(&user.name, &project_name)?;
    }
    let deployment = state
        .deployment_manager
        .deploy(crate_file, project_name)
        .await?;
    Ok(Json(deployment))
}

#[post("/<project_name>/secrets", data = "<secrets>")]
async fn project_secrets(
    state: &State<ApiState>,
    secrets: Json<HashMap<String, String>>,
    project_name: ProjectName,
    user: ScopedUser,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    info!("[PROJECT_SECRETS, {}, {}]", user.name(), &project_name);

    let deployment = state
        .deployment_manager
        .get_deployment_for_project(user.scope())
        .await?;

    if let Some(database_deployment) = &deployment.database_deployment {
        let conn_str = database_deployment.connection_string_private();
        let conn = sqlx::PgPool::connect(&conn_str)
            .await
            .map_err(|e| DeploymentApiError::Internal(e.to_string()))?;

        let map = secrets.into_inner();
        for (key, value) in map.iter() {
            conn.set_secret(key, value)
                .await
                .map_err(|e| DeploymentApiError::BadRequest(e.to_string()))?;
        }
    }

    Ok(Json(deployment))
}

struct ApiState {
    deployment_manager: Arc<DeploymentSystem>,
}

//noinspection ALL
pub async fn rocket() -> Rocket<Build> {
    env_logger::Builder::new()
        .filter_module("rocket", log::LevelFilter::Warn)
        .filter_module("_", log::LevelFilter::Warn)
        .filter_module("shuttle_api", log::LevelFilter::Debug)
        .filter_module("shuttle_service", log::LevelFilter::Debug)
        .init();

    let args: Args = Args::parse();
    let build_system = FsBuildSystem::initialise(args.path).unwrap();
    let deployment_manager = Arc::new(
        DeploymentSystem::new(
            Box::new(build_system),
            args.proxy_fqdn.to_string(),
            args.provisioner_address,
            args.provisioner_port,
        )
        .await,
    );

    start_proxy(args.bind_addr, args.proxy_port, deployment_manager.clone()).await;

    let state = ApiState { deployment_manager };

    let user_directory =
        UserDirectory::from_user_file().expect("could not initialise user directory");

    let config = rocket::Config {
        address: args.bind_addr,
        port: args.api_port,
        ..Default::default()
    };
    rocket::custom(config)
        .mount(
            "/projects",
            routes![
                delete_deployment,
                get_deployment,
                delete_project,
                create_project,
                get_project,
                project_secrets
            ],
        )
        .mount("/", routes![get_or_create_user, status, version])
        .manage(state)
        .manage(user_directory)
}

async fn start_proxy(
    bind_addr: IpAddr,
    proxy_port: Port,
    deployment_manager: Arc<DeploymentSystem>,
) {
    tokio::spawn(async move { proxy::start(bind_addr, proxy_port, deployment_manager).await });
}
