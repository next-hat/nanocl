use ntex::web;

use nanocl_error::http::HttpError;
use nanocl_stubs::job::JobPartial;

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
pub(crate) async fn create_job(
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
  tag = "Jobs",
  path = "/jobs/{Name}",
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
#[web::delete("/jobs/{name}")]
pub(crate) async fn delete_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  utils::job::delete_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Inspect a job
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{Name}/inspect",
  params(
    ("Name" = String, Path, description = "Name of the job"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the job"),
  ),
  responses(
    (status = 200, description = "Cargo details", body = CargoInspect),
  ),
))]
#[web::get("/jobs/{name}/inspect")]
async fn inspect_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let job = utils::job::inspect_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&job))
}

/// Get logs of a job
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{Name}/logs",
  responses(
    (status = 200, description = "Job logs", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Job does not exist"),
  ),
))]
#[web::get("/jobs/{name}/logs")]
async fn logs_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let stream = utils::job::logs_by_name(&path.1, &state).await?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(stream),
  )
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_job);
  config.service(create_job);
  config.service(delete_job);
  config.service(inspect_job);
  config.service(logs_job);
}

#[cfg(test)]
mod tests {
  use ntex::http;
  use nanocl_stubs::job::Job;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/jobs";

  #[ntex::test]
  async fn test_list_jobs() {
    let client = gen_default_test_client().await;
    let mut response = client.get(ENDPOINT).send().await.unwrap();
    test_status_code!(response.status(), http::StatusCode::OK, "list jobs");
    let _ = response.json::<Vec<Job>>().await.unwrap();
  }
}
