use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::job::JobDeleteQuery;

use crate::{
  models::{JobDb, SystemState},
  objects::generic::*,
};

/// Delete a job by name
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Jobs",
  path = "/jobs/{name}",
  params(
    ("name" = String, Path, description = "Name of the job"),
    ("incarnation" = Option<uuid::Uuid>, Query, description = "Expected incarnation; when supplied the current run must also match run, including no run"),
    ("run" = Option<uuid::Uuid>, Query, description = "Expected current run; requires incarnation"),
  ),
  responses(
    (status = 202, description = "Job deletion accepted"),
    (status = 409, description = "Job incarnation or current run changed", body = crate::services::openapi::ApiError),
    (status = 404, description = "Job does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn delete_job(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  query: web::types::Query<JobDeleteQuery>,
) -> HttpResult<web::HttpResponse> {
  query.validate().map_err(HttpError::bad_request)?;
  if let Some(incarnation) = query.incarnation {
    crate::tasks::job::delete_if_identity(
      &path.1,
      incarnation,
      query.run,
      state,
    )
    .await?;
  } else {
    JobDb::del_obj_by_pk(&path.1, &(), &state).await?;
  }
  Ok(web::HttpResponse::Accepted().build())
}
