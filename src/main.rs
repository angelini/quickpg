#[macro_use]
extern crate rocket;

use std::path::Path;

use pg_ctl::Status;
use portpicker;
use rand::distributions::{Alphanumeric, DistString};
use rocket::{response::Responder, serde::json::Json};
use serde::{Deserialize, Serialize};

mod config;
mod pg_ctl;

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

#[derive(Debug, Responder)]
enum ApiError {
    #[response(status = 500, content_type = "json")]
    Internal(Json<InternalError>),
    #[response(status = 404, content_type = "json")]
    NotFound(Json<InstanceId>),
    #[response(status = 502, content_type = "json")]
    TemplateStillRunning(Json<InstanceId>),
}

impl From<pg_ctl::Error> for ApiError {
    fn from(err: pg_ctl::Error) -> Self {
        ApiError::Internal(InternalError::json(format!("pg_ctl: {:?}", err)))
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

#[get("/", format = "json")]
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

#[get("/status/<id>", format = "json")]
async fn status(id: &str) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    Ok(Json(Instance::new(&ctl.user, ctl.status(id).await?)))
}

#[post("/create", data = "<body>", format = "json")]
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

#[post("/start", data = "<body>", format = "json")]
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

#[post("/stop", data = "<body>", format = "json")]
fn stop(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();
    ctl.stop(&body.id)?;
    Ok(Json(()))
}

#[post("/fork", data = "<body>", format = "json")]
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

#[post("/destroy", data = "<body>", format = "json")]
async fn destroy(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();

    let status = ctl.status(&body.id).await?;
    if status.is_running() {
        ctl.stop(&body.id)?;
    }

    ctl.destroy(&body.id).await?;
    Ok(Json(()))
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount(
        "/",
        routes![list, status, create, start, stop, fork, destroy],
    )
}
