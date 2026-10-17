use ntex::web;

pub mod attach;
pub mod count;
pub mod exec;
pub mod inspect;
pub mod kill;
pub mod list;
pub mod log;
mod network;
pub mod restart;
pub mod start;
pub mod stats;
pub mod stop;
pub mod wait;

pub use attach::*;
pub use count::*;
pub use exec::*;
pub use inspect::*;
pub use kill::*;
pub use list::*;
pub use log::*;
pub use restart::*;
pub use start::*;
pub use stats::*;
pub use stop::*;
pub use wait::*;

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  config.service(
    web::resource("/processes/{name}/attach")
      .route(web::get().to_with_state(attach_process)),
  );
  config.service(
    web::resource("/processes/{name}/logs")
      .name("logs_process")
      .guard(web::guard::Get())
      .to_with_state(logs_process),
  );
  config.service(
    web::resource("/processes/{name}/exec")
      .name("create_process_exec")
      .guard(web::guard::Post())
      .to_with_state(create_process_exec),
  );
  config.service(
    web::resource("/exec/{id}/start")
      .route(web::post().to_with_state(start_process_exec_detached))
      .route(web::get().to_with_state(start_process_exec_attached)),
  );
  config.service(
    web::resource("/processes/{name}/inspect")
      .name("inspect_process")
      .guard(web::guard::Get())
      .to_with_state(inspect_process),
  );
  config.service(
    web::resource("/processes")
      .name("list_processes")
      .guard(web::guard::Get())
      .to_with_state(list_processes),
  );
  config.service(
    web::resource("/processes/{kind}/{key}/logs")
      .name("logs_processes")
      .guard(web::guard::Get())
      .to_with_state(logs_processes),
  );
  config.service(
    web::resource("/processes/{name}/kill")
      .name("kill_process")
      .guard(web::guard::Post())
      .to_with_state(kill_process),
  );
  config.service(
    web::resource("/processes/{name}/stop")
      .name("stop_process")
      .guard(web::guard::Post())
      .to_with_state(stop_process),
  );
  config.service(
    web::resource("/processes/{name}/restart")
      .name("restart_process")
      .guard(web::guard::Post())
      .to_with_state(restart_process),
  );
  config.service(
    web::resource("/processes/{name}/wait")
      .name("wait_process")
      .guard(web::guard::Get())
      .to_with_state(wait_process),
  );
  config.service(
    web::resource("/processes/{kind}/{key}/restart")
      .name("restart_processes")
      .guard(web::guard::Post())
      .to_with_state(restart_processes),
  );
  config.service(
    web::resource("/processes/{kind}/{key}/start")
      .name("start_processes")
      .guard(web::guard::Post())
      .to_with_state(start_processes),
  );
  config.service(
    web::resource("/processes/{kind}/{key}/stop")
      .name("stop_processes")
      .guard(web::guard::Post())
      .to_with_state(stop_processes),
  );
  config.service(
    web::resource("/processes/{kind}/{key}/wait")
      .name("wait_processes")
      .guard(web::guard::Get())
      .to_with_state(wait_processes),
  );
  config.service(
    web::resource("/processes/{kind}/{key}/stats")
      .name("stats_processes")
      .guard(web::guard::Get())
      .to_with_state(stats_processes),
  );
  config.service(
    web::resource("/processes/count")
      .name("count_processes")
      .guard(web::guard::Get())
      .to_with_state(count_processes),
  );
  config.service(
    web::resource("/process/{name}/stats")
      .name("process_stats_by_name")
      .guard(web::guard::Get())
      .to_with_state(process_stats_by_name),
  );
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
    let res = client.send_get("/processes", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let _ = res.json::<Vec<Process>>().await.unwrap();
  }

  #[ntex::test]
  async fn test_stats() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_get(
        "/processes/cargo/system.nstore/stats",
        Some(ProcessStatsQuery {
          stream: Some(false),
          one_shot: Some(true),
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo stats");
  }
  #[ntex::test]
  async fn test_process_stats_by_name() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_get("/process/system.nstore.c/stats", None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "process by name stats"
    );
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
    let res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == "system.nstore.c"));
    // Filter by limit and offset
    let filter = GenericFilter::new().limit(1).offset(1);
    let qs = GenericListQuery::try_from(filter).unwrap();
    let res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert_eq!(items.len(), 1);
    // Filter by name and kind
    let filter = GenericFilter::new()
      .r#where("name", GenericClause::Like("system.nstore%".to_owned()))
      .r#where("kind", GenericClause::Eq("cargo".to_owned()));
    let qs = GenericListQuery::try_from(filter).unwrap();
    let res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == "system.nstore.c"));
  }

  #[ntex::test]
  async fn test_inspect() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_get("/processes/system.nstore.c/inspect", None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic process inspect"
    );
  }
}
