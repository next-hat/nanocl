use ntex::web;

pub mod count;
pub mod create;
pub mod inspect;
pub mod list;

pub use count::*;
pub use create::*;
pub use inspect::*;
pub use list::*;

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  config.service(
    web::resource("/metrics")
      .name("list_metric")
      .guard(web::guard::Get())
      .to_with_state(list_metric),
  );
  config.service(
    web::resource("/metrics")
      .name("create_metric")
      .guard(web::guard::Post())
      .to_with_state(create_metric),
  );
  config.service(
    web::resource("/metrics/{key}/inspect")
      .name("inspect_metric")
      .guard(web::guard::Get())
      .to_with_state(inspect_metric),
  );
  config.service(
    web::resource("/metrics/count")
      .name("count_metric")
      .guard(web::guard::Get())
      .to_with_state(count_metric),
  );
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
    let res = client
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
