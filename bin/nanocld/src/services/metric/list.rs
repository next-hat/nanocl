use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{MetricDb, SystemState},
  repositories::generic::*,
  utils,
};

/// List metrics with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Metrics",
  path = "/metrics",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"kind\": { \"eq\": \"CPU\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of metrics", body = Vec<Metric>),
  ),
))]
#[web::get("/metrics")]
pub async fn list_metric(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let metrics = MetricDb::read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&metrics))
}
