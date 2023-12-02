use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::generic::{GenericFilter, GenericListQuery};

use crate::models::{DaemonState, Repository, MetricDb};

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
    HttpError::bad_request(format!("Invalid query string: {}", err))
  })?;
  let metrics = MetricDb::find(&filter, &state.pool).await??;
  Ok(web::HttpResponse::Ok().json(&metrics))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_metric);
}

#[cfg(test)]
mod tests {
  use ntex::http;
  use nanocl_stubs::metric::MetricKind;
  use nanocl_stubs::generic::{GenericFilter, GenericClause, GenericListQuery};

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/metrics";

  async fn test_list(client: &TestClient) {
    let filter = GenericFilter::new()
      .r#where("kind", GenericClause::Eq(MetricKind::Cpu.to_string()));
    let qs = GenericListQuery::try_from(filter).unwrap();
    let res = client.send_get(ENDPOINT, Some(&qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list metrics");
  }

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    test_list(&client).await;
  }
}
