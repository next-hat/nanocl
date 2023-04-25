use ntex::web;

use nanocl_stubs::http_metric::{HttpMetricListQuery, HttpMetricCountQuery};

use crate::repositories;
use nanocl_utils::http_error::HttpError;
use crate::models::DaemonState;

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
) -> Result<web::HttpResponse, HttpError> {
  let metrics = repositories::http_metric::list(&qs, &state.pool).await?;
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
) -> Result<web::HttpResponse, HttpError> {
  let count = repositories::http_metric::count(&qs, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&count))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_http_metric);
  config.service(count_http_metric);
}

#[cfg(test)]
mod tests {

  use ntex::http;
  use nanocl_stubs::generic::GenericCount;
  use nanocl_stubs::http_metric::HttpMetric;

  use crate::services::ntex_config;
  use crate::utils::tests::*;

  async fn test_list(srv: &TestServer) -> TestRet {
    let mut resp = srv.get("/v0.5/http_metrics").send().await?;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let _ = resp.json::<Vec<HttpMetric>>().await?;
    Ok(())
  }

  async fn test_count(srv: &TestServer) -> TestRet {
    let mut resp = srv.get("/v0.5/http_metrics/count").send().await?;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let _ = resp.json::<GenericCount>().await?;
    Ok(())
  }

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;
    test_list(&srv).await?;
    test_count(&srv).await?;
    Ok(())
  }
}
