use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  generic::{GenericCount, GenericListQuery},
  metric::MetricPartial,
};

use crate::{
  models::{MetricDb, MetricNodePartial, SystemState},
  repositories::generic::*,
  utils,
};

/// Get metrics of all peer nodes
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

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_metric);
  config.service(create_metric);
  config.service(inspect_metric);
  config.service(count_metric);
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::{
    generic::{GenericClause, GenericFilter, GenericListQuery},
    metric::{Metric, MetricPartial},
  };
  use ntex::http;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/metrics";

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_post(
        ENDPOINT,
        Some(&MetricPartial {
          kind: "nanocl.io/test".to_owned(),
          data: serde_json::json!({ "test": "test" }),
          note: None,
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "reserved metric kind"
    );
    let mut res = client
      .send_post(
        ENDPOINT,
        Some(&MetricPartial {
          kind: "test.io/test".to_owned(),
          data: serde_json::json!({ "test": "test" }),
          note: None,
        }),
        None::<String>,
      )
      .await;
    let metric = res
      .json::<Metric>()
      .await
      .expect("Expect to parse metrics from post request");
    test_status_code!(res.status(), http::StatusCode::CREATED, "create metric");
    let filter = GenericFilter::new()
      .r#where("kind", GenericClause::Eq("nanocl.io/cpu".to_owned()));
    let qs = GenericListQuery::try_from(filter).unwrap();
    let res = client.send_get(ENDPOINT, Some(&qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list metric");
    let res = client
      .send_get(
        &format!("{}/{}/inspect", ENDPOINT, metric.key),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "inspect metric");
  }
}
