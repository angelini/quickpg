#[macro_use]
extern crate rocket;

use std::path::Path;

use portpicker;
use rocket::serde::{json::Json, Deserialize, Serialize};

mod config;
mod pg_ctl;

#[derive(Debug, Responder)]
enum ApiError {
    #[response(status = 500)]
    PgCtl(pg_ctl::Error),
    #[response(status = 404)]
    NotFound(String),
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

#[post("/start-instance", data = "<body>")]
fn start_instance(body: Json<InstanceId>) -> Result<Json<Instance>> {
    let port: u32 = portpicker::pick_unused_port().unwrap().into();
    let ctl = pg_ctl::PgCtl::new(Path::new("."));

    if !ctl.exists(&body.name) {
        ctl.init(&body.name, &config::PostgresqlConf::default(port))?;
    }

    ctl.start(&body.name)?;

    match ctl.status(&body.name)? {
        Some(pid) => Ok(Json(Instance { pid, port: port })),
        None => Err(ApiError::NotFound(body.name.to_string())),
    }
}

#[post("/stop-instance", data = "<body>")]
fn stop_instance(body: Json<InstanceId>) -> Result<Json<()>> {
    let ctl = pg_ctl::PgCtl::new(Path::new("."));
    ctl.stop(&body.name)?;
    Ok(Json(()))
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct RootStatus<'a> {
    name: &'a str,
    statuses: Vec<(String, Option<u32>)>,
}

#[get("/")]
fn index<'a>() -> Result<Json<RootStatus<'a>>> {
    let ctl = pg_ctl::PgCtl::new(Path::new("."));
    let statuses = ctl.list()?;
    Ok(Json(RootStatus {
        name: "quickpg",
        statuses,
    }))
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![index, start_instance, stop_instance])
}
