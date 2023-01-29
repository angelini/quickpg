#[macro_use]
extern crate rocket;

use std::path::Path;

use portpicker;
use rand::distributions::{Alphanumeric, DistString};
use rocket::{
    response::Responder,
    serde::{json::Json, Deserialize, Serialize},
};

mod config;
mod pg_ctl;

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct InstanceId {
    id: String,
}

impl InstanceId {
    fn json(id: impl Into<String>) -> Json<InstanceId> {
        Json(InstanceId { id: id.into() })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
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
#[serde(crate = "rocket::serde")]
enum InstanceState {
    Stopped,
    Running,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct ProcessInfo {
    pid: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct ConnectionInfo {
    user: String,
    host: String,
    port: u32,
    dbname: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct Instance {
    id: String,
    state: InstanceState,
    conn_info: ConnectionInfo,
    proc_info: Option<ProcessInfo>,
}

impl Instance {
    fn new(id: impl Into<String>, port: u32, pid: Option<u32>) -> Instance {
        let state = match pid {
            Some(_) => InstanceState::Running,
            None => InstanceState::Stopped,
        };
        Instance {
            id: id.into(),
            state,
            conn_info: ConnectionInfo {
                user: whoami::username(),
                host: "127.0.0.1".to_string(),
                port,
                dbname: "postgres".to_string(),
            },
            proc_info: pid.map(|p| ProcessInfo { pid: p }),
        }
    }
}

fn create_ctl() -> pg_ctl::PgCtl {
    pg_ctl::PgCtl::new(Path::new(""))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct ListResponse {
    instances: Vec<Instance>,
}

#[get("/", format = "json")]
fn list() -> Result<Json<ListResponse>> {
    let ctl = create_ctl();
    let instances = ctl.list()?;
    Ok(Json(ListResponse {
        instances: instances
            .into_iter()
            .map(|(id, port, pid)| Instance::new(id, port, pid))
            .collect(),
    }))
}

#[get("/status/<id>", format = "json")]
fn status(id: &str) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    let (port, pid) = ctl.status(id)?;
    Ok(Json(Instance::new(id, port, pid)))
}

#[post("/create", format = "json")]
fn create() -> Result<Json<InstanceId>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let id = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    ctl.init(&id, &config::PostgresqlConf::default(port))?;

    Ok(InstanceId::json(id))
}

#[post("/start", data = "<body>", format = "json")]
fn start(body: Json<InstanceId>) -> Result<Json<Instance>> {
    let ctl = create_ctl();

    if !ctl.exists(&body.id) {
        return Err(ApiError::NotFound(InstanceId::json(&body.id)));
    }

    ctl.start(&body.id)?;

    match ctl.status(&body.id)? {
        (port, Some(pid)) => Ok(Json(Instance::new(&body.id, port, Some(pid)))),
        (_, None) => Err(ApiError::Internal(InternalError::json(format!(
            "not running: {}",
            body.id
        )))),
    }
}

#[post("/stop", data = "<body>", format = "json")]
fn stop(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();
    ctl.stop(&body.id)?;
    Ok(Json(()))
}

#[post("/fork", data = "<body>", format = "json")]
fn fork(body: Json<InstanceId>) -> Result<Json<InstanceId>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let id = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    if !ctl.exists(&body.id) {
        return Err(ApiError::NotFound(InstanceId::json(&body.id)));
    }

    if let (_, Some(_)) = ctl.status(&body.id)? {
        return Err(ApiError::TemplateStillRunning(InstanceId::json(&body.id)));
    }

    ctl.fork(&body.id, &id, &config::PostgresqlConf::default(port))?;

    Ok(InstanceId::json(id))
}

#[post("/destroy", data = "<body>", format = "json")]
fn destroy(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();

    if let (_, Some(_)) = ctl.status(&body.id)? {
        ctl.stop(&body.id)?;
    }

    ctl.destroy(&body.id)?;
    Ok(Json(()))
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount(
        "/",
        routes![list, status, create, start, stop, fork, destroy],
    )
}
