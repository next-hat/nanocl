use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::metric::MetricPartial;

use crate::{
  models::{MetricDb, MetricNodePartial, SystemState},
  repositories::generic::*,
};

/// Create a new metric
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Metrics",
  path = "/metrics",
  request_body = MetricPartial,
  responses(
    (status = 201, description = "Metric created", body = Metric),
  ),
))]
#[web::post("/metrics")]
pub async fn create_metric(
  state: web::types::State<SystemState>,
  _path: web::types::Path<String>,
  payload: web::types::Json<MetricPartial>,
) -> HttpResult<web::HttpResponse> {
  if payload.kind.starts_with("nanocl.io") {
    return Err(HttpError::bad_request("reserved kind nanocl.io"));
  }
  let new_metric =
    MetricNodePartial::try_new_node(&state.inner.config.hostname, &payload)?;
  let metric = MetricDb::create_from(&new_metric, &state.inner.pool).await?;
  Ok(web::HttpResponse::Created().json(&metric))
}
