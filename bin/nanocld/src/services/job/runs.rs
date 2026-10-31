use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::job::JobHistoryQuery;

use crate::models::{JobDb, SystemState};

/// List retained executions of every incarnation of a Job name.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{name}/runs",
  params(
    ("name" = String, Path, description = "Job name, including a removed job"),
    ("limit" = Option<i64>, Query, description = "Page size, 1 to 100; defaults to 50"),
    ("offset" = Option<i64>, Query, description = "Nonnegative offset; defaults to zero"),
  ),
  responses(
    (status = 200, description = "Runs, newest acceptance first", body = Vec<nanocl_stubs::job::JobRun>),
    (status = 400, description = "Invalid pagination", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn list_job_runs(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  query: web::types::Query<JobHistoryQuery>,
) -> HttpResult<web::HttpResponse> {
  query.validate().map_err(HttpError::bad_request)?;
  let runs = JobDb::runs(&path.1, &query, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&runs))
}

/// Inspect a retained execution without requiring the live Job to exist.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{name}/runs/{run}",
  params(
    ("name" = String, Path, description = "Job name, including a removed job"),
    ("run" = uuid::Uuid, Path, description = "Retained execution identity"),
  ),
  responses(
    (status = 200, description = "Pinned definition, initiating operation, and ordered step results; Docker logs are not archived", body = nanocl_stubs::job::JobRunInspect),
    (status = 404, description = "Run does not belong to the named Job", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_job_run(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let run = JobDb::inspect_run(&path.1, path.2, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&run))
}
