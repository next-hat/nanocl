use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  metric::MetricPartial,
  generic::{GenericFilter, GenericListQuery},
};

use crate::{
  repositories::generic::*,
  models::{DaemonState, MetricDb, MetricNodePartial},
};

/// Get metrics of all peer nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Metrics",
  path = "/metrics",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"kind\": { \"eq\": \"CPU\" } } }"),
  ),
  responses(
    (status = 200, description = "List of metrics", body = Vec<Metric>),
  ),
))]
#[web::get("/metrics")]
pub(crate) async fn list_metric(
  state: web::types::State<DaemonState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(qs.into_inner()).map_err(|err| {
    HttpError::bad_request(format!("Invalid query string: {err}"))
  })?;
  let metrics = MetricDb::read_by(&filter, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&metrics))
}

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
pub(crate) async fn create_metric(
  state: web::types::State<DaemonState>,
  _path: web::types::Path<String>,
  payload: web::types::Json<MetricPartial>,
) -> HttpResult<web::HttpResponse> {
  if payload.kind.starts_with("nanocl.io") {
    return Err(HttpError::bad_request("reserved kind nanocl.io"));
  }
  let new_metric =
    MetricNodePartial::try_new_node(&state.config.hostname, &payload)?;
  let metric = MetricDb::create_from(&new_metric, &state.pool).await?;
  Ok(web::HttpResponse::Created().json(&metric))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_metric);
  config.service(create_metric);
}

#[cfg(test)]
mod tests {
  use ntex::http;
  use nanocl_stubs::{
    metric::MetricPartial,
    generic::{GenericFilter, GenericClause, GenericListQuery},
  };

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/metrics";

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    let res = client
      .send_post(
        ENDPOINT,
        Some(&MetricPartial {
          kind: "nanocl.io/test".to_owned(),
          data: serde_json::json!({ "test": "test" }),
          display: None,
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "reserved metric kind"
    );
    let res = client
      .send_post(
        ENDPOINT,
        Some(&MetricPartial {
          kind: "test.io/test".to_owned(),
          data: serde_json::json!({ "test": "test" }),
          display: None,
        }),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create metric");
    let filter = GenericFilter::new()
      .r#where("kind", GenericClause::Eq("nanocl.io/cpu".to_owned()));
    let qs = GenericListQuery::try_from(filter).unwrap();
    let res = client.send_get(ENDPOINT, Some(&qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list metric");
  }
}
