use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{NetworkDb, SystemState},
  repositories::generic::*,
  utils,
};

/// List persisted node-scoped Docker network snapshots.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Networks",
  path = "/networks",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"node_name\": { \"eq\": \"node-a\" } } }"),
  ),
  responses(
    (status = 200, description = "List of networks", body = [nanocl_stubs::network::Network]),
  ),
))]
#[web::get("/networks")]
pub async fn list_network(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let networks =
    NetworkDb::transform_read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&networks))
}
