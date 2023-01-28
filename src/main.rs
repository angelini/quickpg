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
struct Instance {
    pid: u32,
    port: u32,
}

fn create_ctl() -> pg_ctl::PgCtl {
    pg_ctl::PgCtl::new(Path::new(""))
}

#[post("/create")]
fn create() -> Result<Json<InstanceId>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let name = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    ctl.init(&name, &config::PostgresqlConf::default(port))?;

    Ok(InstanceId::json(name))
}

#[post("/start", data = "<body>")]
fn start(body: Json<InstanceId>) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();

    if !ctl.exists(&body.name) {
        return Err(ApiError::NotFound(InstanceId::json(&body.name)));
    }

    ctl.start(&body.name)?;

    match ctl.status(&body.name)? {
        Some(pid) => Ok(Json(Instance { pid, port })),
        None => Err(ApiError::Internal(InternalError::json(format!(
            "not running: {}",
            body.name
        )))),
    }
}

#[post("/stop", data = "<body>")]
fn stop(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();
    ctl.stop(&body.name)?;
    Ok(Json(()))
}

#[post("/fork", data = "<body>")]
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

#[post("/destroy", data = "<body>")]
fn destroy(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = create_ctl();

    if let Some(_) = ctl.status(&body.name)? {
        ctl.stop(&body.name)?;
    }

    ctl.destroy(&body.name)?;
    Ok(Json(()))
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct RootStatus<'a> {
    name: &'a str,
    instances: Vec<(String, Option<u32>)>,
}

#[get("/")]
fn index<'a>() -> Result<Json<RootStatus<'a>>> {
    let ctl = create_ctl();
    let instances = ctl.list()?;
    Ok(Json(RootStatus {
        name: "quickpg",
        instances,
    }))
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![index, create, start, stop, fork, destroy])
}
