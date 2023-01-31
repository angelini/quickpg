mod config;
mod pg_ctl;

use std::path::Path;

use axum::{
    extract::Path as UriPath, http::StatusCode, response::IntoResponse, routing, Json, Router,
};
use portpicker;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use serde_json::json;

use pg_ctl::Status;

#[derive(Debug, Deserialize, Serialize)]
struct InstanceId {
    id: String,
}

impl InstanceId {
    fn json(id: impl Into<String>) -> Json<InstanceId> {
        Json(InstanceId { id: id.into() })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct InstanceDescriptor {
    dbname: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct InternalError {
    message: String,
}

impl InternalError {
    fn json(message: impl Into<String>) -> Json<InternalError> {
        Json(InternalError {
            message: message.into(),
        })
    }
}

#[derive(Debug)]
enum ApiError {
    Internal(Json<InternalError>),
    NotFound(Json<InstanceId>),
    TemplateStillRunning(Json<InstanceId>),
}

impl From<pg_ctl::Error> for ApiError {
    fn from(err: pg_ctl::Error) -> Self {
        ApiError::Internal(InternalError::json(format!("pg_ctl: {:?}", err)))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            ApiError::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal error: {}", err.message),
            ),
            ApiError::NotFound(id) => (StatusCode::NOT_FOUND, format!("Not found: {}", id.id)),
            ApiError::TemplateStillRunning(id) => (
                StatusCode::BAD_REQUEST,
                format!("Instance {} is still running", id.id),
            ),
        };

        let body = Json(json!({ "error": message }));

        (status, body).into_response()
    }
}

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Deserialize, Serialize)]
enum InstanceState {
    Stopped,
    Running,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProcessInfo {
    pid: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct ConnectionInfo {
    user: String,
    host: String,
    port: u32,
    dbname: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Instance {
    id: String,
    state: InstanceState,
    conn_info: ConnectionInfo,
    proc_info: Option<ProcessInfo>,
}

impl Instance {
    fn new(user: impl Into<String>, status: Status) -> Instance {
        let state = match status.pid {
            Some(_) => InstanceState::Running,
            None => InstanceState::Stopped,
        };
        Instance {
            id: status.id,
            state,
            conn_info: ConnectionInfo {
                user: user.into(),
                host: "127.0.0.1".to_string(),
                port: status.port,
                dbname: status.dbname,
            },
            proc_info: status.pid.map(|p| ProcessInfo { pid: p }),
        }
    }
}

fn create_ctl() -> pg_ctl::PgCtl {
    pg_ctl::PgCtl::new(whoami::username(), Path::new(""))
}

#[derive(Debug, Deserialize, Serialize)]
struct ListResponse {
    instances: Vec<Instance>,
}

async fn list() -> Result<Json<ListResponse>> {
    let ctl = create_ctl();
    let instances = ctl.list().await?;
    Ok(Json(ListResponse {
        instances: instances
            .into_iter()
            .map(|status| Instance::new(&ctl.user, status))
            .collect(),
    }))
}

async fn status(id: UriPath<String>) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    Ok(Json(Instance::new(&ctl.user, ctl.status(&id).await?)))
}

async fn create(body: Json<InstanceDescriptor>) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let id = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    ctl.init(&id, &body.dbname, &config::PostgresqlConf::default(port))
        .await?;

    let status = ctl.status(&id).await?;
    if !status.is_running() {
        return Err(ApiError::Internal(InternalError::json(format!(
            "did not start: {}",
            id
        ))));
    }

    Ok(Json(Instance::new(&ctl.user, status)))
}

async fn start(body: Json<InstanceId>) -> Result<Json<Instance>> {
    let ctl = create_ctl();

    if !ctl.exists(&body.id) {
        return Err(ApiError::NotFound(InstanceId::json(&body.id)));
    }

    ctl.start(&body.id)?;

    let status = ctl.status(&body.id).await?;
    if !status.is_running() {
        return Err(ApiError::Internal(InternalError::json(format!(
            "not running: {}",
            body.id
        ))));
    }

    Ok(Json(Instance::new(&ctl.user, status)))
}

async fn stop(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();
    ctl.stop(&body.id)?;
    Ok(Json(()))
}

async fn fork(body: Json<InstanceId>) -> Result<Json<InstanceId>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let id = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    if !ctl.exists(&body.id) {
        return Err(ApiError::NotFound(InstanceId::json(&body.id)));
    }

    let status = ctl.status(&body.id).await?;
    if status.is_running() {
        return Err(ApiError::TemplateStillRunning(InstanceId::json(&body.id)));
    }

    ctl.fork(
        &body.id,
        &id,
        &status.dbname,
        &config::PostgresqlConf::default(port),
    )
    .await?;

    Ok(InstanceId::json(id))
}

async fn destroy(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();

    let status = ctl.status(&body.id).await?;
    if status.is_running() {
        ctl.stop(&body.id)?;
    }

    ctl.destroy(&body.id).await?;
    Ok(Json(()))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", routing::get(list))
        .route("/status/:id", routing::get(status))
        .route("/create", routing::post(create))
        .route("/start", routing::post(start))
        .route("/stop", routing::post(stop))
        .route("/fork", routing::post(fork))
        .route("/destroy", routing::post(destroy));

    axum::Server::bind(&"0.0.0.0:8000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
