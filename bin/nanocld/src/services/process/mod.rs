use ntex::web;

pub mod count;
pub mod kill;
pub mod list;
pub mod log;
pub mod restart;
pub mod start;
pub mod stats;
pub mod stop;
pub mod wait;

pub use count::*;
pub use kill::*;
pub use list::*;
pub use log::*;
pub use restart::*;
pub use start::*;
pub use stats::*;
pub use stop::*;
pub use wait::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_processes);
  config.service(logs_processes);
  config.service(logs_process);
  config.service(restart_processes);
  config.service(start_processes);
  config.service(stop_processes);
  config.service(kill_processes);
  config.service(wait_processes);
  config.service(stats_processes);
  config.service(count_process);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use crate::utils::tests::*;

  use nanocl_stubs::{
    generic::{GenericClause, GenericFilter, GenericListQuery},
    process::{Process, ProcessStatsQuery},
  };

  #[ntex::test]
  async fn basic_list() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut res = client.send_get("/processes", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let _ = res.json::<Vec<Process>>().await.unwrap();
  }

  #[ntex::test]
  async fn test_stats() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_get(
        "/processes/cargo/nstore/stats",
        Some(ProcessStatsQuery {
          namespace: Some("system".to_owned()),
          stream: Some(false),
          one_shot: Some(true),
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo stats");
  }

  #[ntex::test]
  async fn list_by() {
    let system = gen_default_test_system().await;
    let client = system.client;
    // Filter by namespace
    let filter = GenericFilter::new().r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "Config": {
          "Labels": {
            "io.nanocl.n": "system",
          }
        }
      })),
    );
    let qs = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == "nstore.system.c"));
    // Filter by limit and offset
    let filter = GenericFilter::new().limit(1).offset(1);
    let qs = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert_eq!(items.len(), 1);
    // Filter by name and kind
    let filter = GenericFilter::new()
      .r#where("name", GenericClause::Like("nstore%".to_owned()))
      .r#where("kind", GenericClause::Eq("cargo".to_owned()));
    let qs = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == "nstore.system.c"));
  }
}
