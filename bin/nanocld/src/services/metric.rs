use ntex::web;

use nanocl_stubs::metric::MetricFilterQuery;

use crate::models::DaemonState;

use crate::repositories;
use nanocl_utils::http_error::HttpError;

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

  use nanocl_utils::ntex::test_client::{TestClient, test_status_code};

  use crate::version::VERSION;
  use crate::services::ntex_config;
  use crate::utils::tests::*;

  const ENDPOINT: &str = "/metrics";

  async fn test_list(client: &TestClient) {
    let mut res = client
      .send_get(
        ENDPOINT,
        Some(&MetricFilterQuery {
          kind: MetricKind::Cpu,
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "list metrics");
    let _ = res.json::<Vec<Metric>>().await.unwrap();
  }

  #[ntex::test]
  async fn basic() {
    let client = generate_test_client(ntex_config, VERSION).await;
    test_list(&client).await;
  }
}
