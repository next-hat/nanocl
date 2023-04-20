use ntex::web;

use nanocl_stubs::metric::MetricFilterQuery;

use crate::models::DaemonState;

use crate::repositories;
use crate::error::HttpError;

/// Get specific metric of all peer nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Metrics",
  path = "/metrics",
  params(
    ("Kind" = String, Query, description = "Kind of the metrics CPU|MEMORY|NETWORK|DISK", example = "CPU"),
  ),
  responses(
    (status = 200, description = "Kind of the metrics peer node", body = Vec<Metric>),
  ),
))]
#[web::get("/metrics")]
pub(crate) async fn list_metric(
  qs: web::types::Query<MetricFilterQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let metrics =
    repositories::metric::list_by_kind(&qs.kind, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&metrics))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_metric);
}
