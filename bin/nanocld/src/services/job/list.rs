use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{JobDb, SystemState},
  utils,
};

/// List jobs with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"job-example\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of jobs", body = [JobSummary]),
  ),
))]
#[web::get("/jobs")]
pub async fn list_job(
  state: web::types::State<SystemState>,
  _version: web::types::Path<String>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let jobs = JobDb::list(&filter, &state).await?;
  Ok(web::HttpResponse::Ok().json(&jobs))
}
