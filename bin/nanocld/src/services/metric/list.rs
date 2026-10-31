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
    (status = 200, description = "List of metrics", body = Vec<nanocl_stubs::metric::Metric>),
  ),
))]
pub async fn list_metric(
  state: &SystemState,
  _request_state: (),
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let metrics = MetricDb::read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&metrics))
}
