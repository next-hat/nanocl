use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{ResourceDb, SystemState},
  repositories::generic::*,
  utils,
};

/// List resources with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"kind\": { \"eq\": \"ncproxy.io/rule\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of resources", body = [Resource]),
  ),
))]
#[web::get("/resources")]
pub async fn list_resource(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let items = ResourceDb::transform_read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}
