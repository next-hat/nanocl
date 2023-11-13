use nanocl_stubs::job::JobPartial;
use ntex::web;

use nanocl_error::http::HttpError;

use crate::utils;
use crate::models::DaemonState;

/// List jobs
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs",
  responses(
    (status = 200, description = "List of jobs", body = [Job]),
  ),
))]
#[web::get("/jobs")]
pub(crate) async fn list_job(
  state: web::types::State<DaemonState>,
  _version: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpError> {
  let jobs = utils::job::list(&state).await?;
  Ok(web::HttpResponse::Ok().json(&jobs))
}

/// Create a job
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Jobs",
  path = "/jobs",
  request_body = JobPartial,
  responses(
    (status = 201, description = "Cargo created", body = Job),
  ),
))]
#[web::post("/jobs")]
pub(crate) async fn create_jobs(
  web::types::Json(payload): web::types::Json<JobPartial>,
  state: web::types::State<DaemonState>,
  _version: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpError> {
  let job = utils::job::create(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&job))
}

/// Delete a job
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Cargoes",
  path = "/jobes/{Name}",
  params(
    ("Name" = String, Path, description = "Name of the job"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the job"),
    ("Force" = bool, Query, description = "If true forces the delete operation"),
  ),
  responses(
    (status = 202, description = "Cargo deleted"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::delete("/jobes/{name}")]
pub(crate) async fn delete_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  utils::job::delete_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_job);
  config.service(create_jobs);
  config.service(delete_job);
}
