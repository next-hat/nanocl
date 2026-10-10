use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::job::JobPartial;

use crate::{
  models::{JobDb, SystemState},
  objects::generic::*,
};

/// Create a new job
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Jobs",
  path = "/jobs",
  request_body = JobPartial,
  responses(
    (status = 201, description = "Job created", body = nanocl_stubs::job::Job),
    (status = 409, description = "Job already exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn create_job(
  state: &SystemState,
  _request_state: (),
  _version: web::types::Path<String>,
  payload: web::types::Json<JobPartial>,
) -> HttpResult<web::HttpResponse> {
  let job = JobDb::create_obj(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&job))
}
