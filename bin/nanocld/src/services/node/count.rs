use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericCount, GenericListQuery};

use crate::{
  models::{NodeDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Count nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Nodes",
  path = "/nodes/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"test\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of nodes", body = [Node]),
  ),
))]
#[web::get("/nodes/count")]
pub async fn count_node(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let count = NodeDb::count_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
