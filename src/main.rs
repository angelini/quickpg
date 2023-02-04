mod config;
mod copy;
mod pg_ctl;

use axum::{extract::Path, http::StatusCode, response::IntoResponse, routing, Json, Router};
use portpicker;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use serde_json::json;

use pg_ctl::Status;
use tower_http::trace::TraceLayer;

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

#[derive(Debug)]
enum ApiError {
    PgCtl(pg_ctl::Error),
    NotFound(Json<InstanceId>),
    FailedToStart(Json<InstanceId>),
    TemplateStillRunning(Json<InstanceId>),
}

impl From<pg_ctl::Error> for ApiError {
    fn from(err: pg_ctl::Error) -> Self {
        ApiError::PgCtl(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            ApiError::PgCtl(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("pg_ctl: {:?}", err),
            ),
            ApiError::NotFound(id) => (StatusCode::NOT_FOUND, format!("Not found: {}", id.id)),
            ApiError::FailedToStart(id) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Instance {} failed to start", id.id),
            ),
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
    pg_ctl::PgCtl::new(whoami::username(), std::path::Path::new(""))
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

async fn create(body: Json<InstanceDescriptor>) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let id = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    ctl.init(&id, &body.dbname, &config::PostgresqlConf::default(port))
        .await?;

    let status = ctl.status(&id).await?;
    if !status.is_running() {
        return Err(ApiError::FailedToStart(InstanceId::json(id)));
    }

    Ok(Json(Instance::new(&ctl.user, status)))
}

async fn status(Path(id): Path<String>) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    Ok(Json(Instance::new(&ctl.user, ctl.status(&id).await?)))
}

async fn start(Path(id): Path<String>) -> Result<Json<Instance>> {
    let ctl = create_ctl();

    if !ctl.exists(&id) {
        return Err(ApiError::NotFound(InstanceId::json(id)));
    }

    ctl.start(&id).await?;

    let status = ctl.status(&id).await?;
    if !status.is_running() {
        return Err(ApiError::FailedToStart(InstanceId::json(id)));
    }

    Ok(Json(Instance::new(&ctl.user, status)))
}

async fn stop(Path(id): Path<String>) -> Result<Json<()>> {
    let ctl = create_ctl();
    ctl.stop(&id, true).await?;
    Ok(Json(()))
}

async fn fork(Path(template): Path<String>) -> Result<Json<Instance>> {
    let ctl = create_ctl();

    if !ctl.exists(&template) {
        return Err(ApiError::NotFound(InstanceId::json(&template)));
    }

    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let id = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    let template_status = ctl.status(&template).await?;
    if template_status.is_running() {
        return Err(ApiError::TemplateStillRunning(InstanceId::json(&template)));
    }

    ctl.fork(
        &template,
        &id,
        &template_status.dbname,
        &config::PostgresqlConf::default(port),
    )
    .await?;

    let status = ctl.status(&id).await?;
    if !status.is_running() {
        return Err(ApiError::FailedToStart(InstanceId::json(id)));
    }

    Ok(Json(Instance::new(&ctl.user, status)))
}

async fn destroy(Path(id): Path<String>) -> Result<Json<()>> {
    let ctl = create_ctl();

    if ctl.is_running(&id) {
        ctl.stop(&id, false).await?;
    }

    ctl.destroy(&id).await?;
    Ok(Json(()))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let app = Router::new()
        .route("/pg/instance", routing::get(list))
        .route("/pg/instance", routing::post(create))
        .route("/pg/instance/:id", routing::get(status))
        .route("/pg/instance/:id/start", routing::post(start))
        .route("/pg/instance/:id/stop", routing::post(stop))
        .route("/pg/instance/:id/fork", routing::post(fork))
        .route("/pg/instance/:id", routing::delete(destroy))
        .layer(TraceLayer::new_for_http());

    axum::Server::bind(&"0.0.0.0:8000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
