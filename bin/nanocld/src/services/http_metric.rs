use nanocl_stubs::generic::GenericFilter;
use ntex::web;

use nanocl_error::http::HttpResult;

use nanocl_stubs::http_metric::{HttpMetricListQuery, HttpMetricCountQuery};

use crate::models::{DaemonState, HttpMetricDb, Repository};

/// Get http metrics of all peer nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "HttpMetrics",
  path = "/http_metrics",
  params(
    ("Limit" = Option<i64>, Query, description = "Limit of the list"),
    ("Offset" = Option<i64>, Query, description = "Offset of the list"),
  ),
  responses(
    (status = 200, description = "Array of HTTP metrics founds", body = Vec<HttpMetric>),
  ),
))]
#[web::get("/http_metrics")]
pub(crate) async fn list_http_metric(
  qs: web::types::Query<HttpMetricListQuery>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
  let metrics =
    HttpMetricDb::find(&GenericFilter::default(), &state.pool).await??;
  Ok(web::HttpResponse::Ok().json(&metrics))
}

/// Count http metrics of all peer nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "HttpMetrics",
  path = "/http_metrics/count",
  params(
    ("Status" = Option<String>, Query, description = "Filter by status", example = "200,299"),
  ),
  responses(
    (status = 200, description = "Count of HTTP metrics founds", body = GenericCount),
  ),
))]
#[web::get("/http_metrics/count")]
pub(crate) async fn count_http_metric(
  qs: web::types::Query<HttpMetricCountQuery>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
  // let count = repositories::http_metric::count(&qs, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&serde_json::Value::default()))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_http_metric);
  config.service(count_http_metric);
}

#[cfg(test)]
mod tests {
  use ntex::http;
  use nanocl_stubs::http_metric::HttpMetric;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/http_metrics";

  async fn test_list(client: &TestClient) {
    let mut resp = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(resp.status(), http::StatusCode::OK, "list http metrics");
    let _ = resp.json::<Vec<HttpMetric>>().await.unwrap();
  }

  // async fn test_count(client: &TestClient) {
  //   let mut resp = client
  //     .send_get(&format!("{ENDPOINT}/count"), None::<String>)
  //     .await;
  //   test_status_code!(
  //     resp.status(),
  //     http::StatusCode::OK,
  //     "count http metrics"
  //   );
  //   let _ = resp.json::<GenericCount>().await.unwrap();
  // }

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    test_list(&client).await;
    // test_count(&client).await;
  }
}
