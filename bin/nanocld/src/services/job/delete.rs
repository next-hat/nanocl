use ntex::web;

use nanocl_error::http::HttpResult;

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
  ),
  responses(
    (status = 202, description = "Job deleted"),
    (status = 404, description = "Job does not exist"),
  ),
))]
#[web::delete("/jobs/{name}")]
pub async fn delete_job(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  JobDb::del_obj_by_pk(&path.1, &(), &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}
