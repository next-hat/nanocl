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
    ("Kind" = MetricKind, Query, description = "Kind of the metrics CPU | MEMORY | NETWORK | DISK", example = "CPU"),
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
    repositories::metric::list_by_kind(&qs.kind.to_string(), &state.pool)
      .await?;
  Ok(web::HttpResponse::Ok().json(&metrics))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_metric);
}

#[cfg(test)]
mod tests {

  use ntex::http;
  use nanocl_stubs::metric::{Metric, MetricKind, MetricFilterQuery};

  use crate::services::ntex_config;
  use crate::utils::tests::*;

  async fn test_list(srv: &TestServer) -> TestRet {
    let mut resp = srv
      .get("/v0.5/metrics")
      .query(&MetricFilterQuery {
        kind: MetricKind::Cpu,
      })
      .unwrap()
      .send()
      .await?;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let _ = resp.json::<Vec<Metric>>().await?;
    Ok(())
  }

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;
    test_list(&srv).await?;
    Ok(())
  }
}
