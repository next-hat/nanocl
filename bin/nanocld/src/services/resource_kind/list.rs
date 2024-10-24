use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{ResourceKindDb, SystemState},
  repositories::generic::*,
  utils,
};

/// List resource kinds
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "ResourceKinds",
  path = "/resource/kinds",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"test\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of jobs", body = [ResourceKind]),
  ),
))]
#[web::get("/resource/kinds")]
pub async fn list_resource_kind(
  state: web::types::State<SystemState>,
  _version: web::types::Path<String>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let resource_kinds =
    ResourceKindDb::transform_read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&resource_kinds))
}
