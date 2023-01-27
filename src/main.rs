#[macro_use]
extern crate rocket;

use std::path::Path;

use portpicker;
use rand::distributions::{Alphanumeric, DistString};
use rocket::serde::{json::Json, Deserialize, Serialize};

mod config;
mod pg_ctl;

#[derive(Debug, Responder)]
enum ApiError {
    #[response(status = 500)]
    PgCtl(pg_ctl::Error),
    #[response(status = 404)]
    NotFound(String),
    #[response(status = 500)]
    NotRunning(String),
    #[response(status = 400)]
    TemplateStillRunning(String),
}

impl From<pg_ctl::Error> for ApiError {
    fn from(err: pg_ctl::Error) -> Self {
        ApiError::PgCtl(err)
    }
}

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct InstanceId {
    name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct Instance {
    pid: u32,
    port: u32,
}

fn create_ctl() -> pg_ctl::PgCtl {
    pg_ctl::PgCtl::new(Path::new("."))
}

#[post("/create")]
fn create() -> Result<Json<InstanceId>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let name = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);

    ctl.init(&name, &config::PostgresqlConf::default(port))?;

    Ok(Json(InstanceId { name }))
}

#[post("/start", data = "<body>")]
fn start(body: Json<InstanceId>) -> Result<Json<Instance>> {
    let ctl = create_ctl();
    let port: u32 = portpicker::pick_unused_port().unwrap().into();

    if !ctl.exists(&body.name) {
        return Err(ApiError::NotFound(body.name.to_string()));
    }

    ctl.start(&body.name)?;

    match ctl.status(&body.name)? {
        Some(pid) => Ok(Json(Instance { pid, port })),
        None => Err(ApiError::NotRunning(body.name.to_string())),
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
        return Err(ApiError::NotFound(body.name.to_string()));
    }

    if let Some(_) = ctl.status(&body.name)? {
        return Err(ApiError::TemplateStillRunning(body.name.to_string()));
    }

    ctl.fork(&body.name, &name, &config::PostgresqlConf::default(port))?;

    Ok(Json(InstanceId { name }))
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct RootStatus<'a> {
    name: &'a str,
    statuses: Vec<(String, Option<u32>)>,
}

#[get("/")]
fn index<'a>() -> Result<Json<RootStatus<'a>>> {
    let ctl = create_ctl();
    let statuses = ctl.list()?;
    Ok(Json(RootStatus {
        name: "quickpg",
        statuses,
    }))
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![index, create, start, stop, fork])
}
