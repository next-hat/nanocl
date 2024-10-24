use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{JobDb, SystemState},
  objects::generic::*,
};

/// Get detailed information about a job
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{name}/inspect",
  params(
    ("name" = String, Path, description = "Name of the job"),
  ),
  responses(
    (status = 200, description = "Job details", body = JobInspect),
  ),
))]
#[web::get("/jobs/{name}/inspect")]
pub async fn inspect_job(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let job = JobDb::inspect_obj_by_pk(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&job))
}
