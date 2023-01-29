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
    name: String,
}

impl InstanceId {
    fn json<S: Into<String>>(name: S) -> Json<InstanceId> {
        Json(InstanceId { name: name.into() })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct InternalError {
    message: String,
}

impl InternalError {
    fn json<S: Into<String>>(message: S) -> Json<InternalError> {
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
    name: String,
    state: InstanceState,
    conn_info: ConnectionInfo,
    proc_info: Option<ProcessInfo>,
}

impl Instance {
    fn new(name: impl Into<String>, port: u32, pid: Option<u32>) -> Instance {
        let state = match pid {
            Some(_) => InstanceState::Running,
            None => InstanceState::Stopped,
        };
        Instance {
            name: name.into(),
            state,
            conn_info: ConnectionInfo {
                user: "".to_string(),
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
            .map(|(name, port, pid)| Instance::new(name, port, pid))
            .collect(),
    }))
}

#[get("/status/<name>", format = "json")]
fn status(name: &str) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    let (port, pid) = ctl.get(name)?;
    Ok(Json(Instance::new(name, port, pid)))
}

#[post("/create", format = "json")]
fn create() -> Result<Json<InstanceId>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let name = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    ctl.init(&name, &config::PostgresqlConf::default(port))?;

    Ok(InstanceId::json(name))
}

#[post("/start", data = "<body>", format = "json")]
fn start(body: Json<InstanceId>) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();

    if !ctl.exists(&body.name) {
        return Err(ApiError::NotFound(InstanceId::json(&body.name)));
    }

    ctl.start(&body.name)?;

    match ctl.status(&body.name)? {
        Some(pid) => Ok(Json(Instance::new(&body.name, port, Some(pid)))),
        None => Err(ApiError::Internal(InternalError::json(format!(
            "not running: {}",
            body.name
        )))),
    }
}

#[post("/stop", data = "<body>", format = "json")]
fn stop(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();
    ctl.stop(&body.name)?;
    Ok(Json(()))
}

#[post("/fork", data = "<body>", format = "json")]
fn fork(body: Json<InstanceId>) -> Result<Json<InstanceId>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let name = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    if !ctl.exists(&body.name) {
        return Err(ApiError::NotFound(InstanceId::json(&body.name)));
    }

    if let Some(_) = ctl.status(&body.name)? {
        return Err(ApiError::TemplateStillRunning(InstanceId::json(&body.name)));
    }

    ctl.fork(&body.name, &name, &config::PostgresqlConf::default(port))?;

    Ok(Json(InstanceId { name }))
}

#[post("/destroy", data = "<body>", format = "json")]
fn destroy(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();

    if let Some(_) = ctl.status(&body.name)? {
        ctl.stop(&body.name)?;
    }

    ctl.destroy(&body.name)?;
    Ok(Json(()))
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount(
        "/",
        routes![list, status, create, start, stop, fork, destroy],
    )
}
