use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericCount, GenericListQuery};

use crate::{
  models::{MetricDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Count metrics
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Metrics",
  path = "/metrics/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"kind\": { \"eq\": \"ncproxy.io/http\" } } } }"),
  ),
  responses(
    (status = 200, description = "Count result", body = GenericCount),
  ),
))]
#[web::get("/metrics/count")]
pub async fn count_metric(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let count = MetricDb::count_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
