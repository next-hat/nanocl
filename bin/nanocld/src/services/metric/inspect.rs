use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{MetricDb, SystemState},
  repositories::generic::*,
};

/// Get detailed information about a metric
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Metrics",
  path = "/metrics/{key}/inspect",
  params(
    ("key" = String, Path, description = "Key of the metric"),
  ),
  responses(
    (status = 200, description = "Detailed information about a metric", body = Metric),
  ),
))]
#[web::get("/metrics/{key}/inspect")]
pub async fn inspect_metric(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let metric = MetricDb::read_by_pk(&path.1, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&metric))
}
