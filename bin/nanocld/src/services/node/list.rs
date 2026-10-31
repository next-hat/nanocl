use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{NodeDb, SystemState},
  repositories::generic::*,
  utils,
};

/// List nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Nodes",
  path = "/nodes",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"test\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of nodes", body = [nanocl_stubs::node::Node]),
  ),
))]
pub async fn list_node(
  state: &SystemState,
  _request_state: (),
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let items = NodeDb::read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}
