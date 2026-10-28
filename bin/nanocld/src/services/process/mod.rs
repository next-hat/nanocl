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

/// A managed Job container is one immutable step, not a restartable service.
/// Treat incomplete ownership metadata conservatively as well.
fn reject_job_container_action(
  runtime: &bollard_next::models::ContainerInspectResponse,
) -> nanocl_error::http::HttpResult<()> {
  let labels = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  if labels.is_some_and(|labels| {
    labels
      .get("io.nanocl.kind")
      .is_some_and(|kind| kind == "job")
      || labels.contains_key("io.nanocl.j")
      || labels.contains_key("io.nanocl.job.run")
  }) {
    return Err(nanocl_error::http::HttpError::conflict(
      "Individual Job steps cannot be controlled; use nanocl job stop or start for the whole Job",
    ));
  }
  Ok(())
}

/// Identify a managed VM before choosing a raw Docker lifecycle operation.
/// Ambiguous VM metadata must never fall back to a Docker power cut.
fn managed_vm_runtime_key(
  runtime: &bollard_next::models::ContainerInspectResponse,
  known_vm: bool,
) -> nanocl_error::http::HttpResult<Option<String>> {
  use nanocl_error::http::HttpError;
  let labels = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  let label = |name: &str| {
    labels
      .and_then(|labels| labels.get(name))
      .map(String::as_str)
  };
  if label("io.nanocl.init-v").is_some() {
    return Ok(None);
  }
  let key = label("io.nanocl.v");
  if !known_vm && key.is_none() && label("io.nanocl.kind") != Some("vm") {
    return Ok(None);
  }
  let key = key.ok_or_else(|| {
    HttpError::conflict("VM runtime has no canonical owner key")
  })?;
  let parsed = key
    .parse::<nanocl_stubs::resource_key::ResourceKey>()
    .map_err(|_| HttpError::conflict("VM runtime has an invalid owner key"))?;
  if runtime.name.as_deref() != Some(format!("/{parsed}.v").as_str()) {
    return Err(HttpError::conflict(
      "VM runtime name does not match its owner key",
    ));
  }
  Ok(Some(parsed.to_string()))
}

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

  #[test]
  fn managed_job_container_actions_reject_partial_and_complete_ownership() {
    for labels in [
      serde_json::json!({"io.nanocl.kind": "job"}),
      serde_json::json!({"io.nanocl.j": "backup"}),
      serde_json::json!({"io.nanocl.job.run": "run"}),
    ] {
      let runtime = serde_json::from_value(serde_json::json!({
        "Config": {"Labels": labels}
      }))
      .unwrap();
      let error = super::reject_job_container_action(&runtime).unwrap_err();
      assert_eq!(error.status, ntex::http::StatusCode::CONFLICT);
    }
    assert!(super::reject_job_container_action(&Default::default()).is_ok());
    let cargo = serde_json::from_value(serde_json::json!({
      "Config": {"Labels": {"io.nanocl.kind": "cargo"}}
    }))
    .unwrap();
    assert!(super::reject_job_container_action(&cargo).is_ok());
  }

  #[test]
  fn managed_vm_lifecycle_classification_rejects_ambiguous_runtime() {
    let runtime = |name: &str, labels: serde_json::Value| {
      serde_json::from_value::<bollard_next::models::ContainerInspectResponse>(
        serde_json::json!({"Name": name, "Config": {"Labels": labels}}),
      )
      .unwrap()
    };
    let vm = runtime(
      "/global.guest.v",
      serde_json::json!({"io.nanocl.v":"global.guest", "io.nanocl.kind":"vm"}),
    );
    assert_eq!(
      super::managed_vm_runtime_key(&vm, false)
        .unwrap()
        .as_deref(),
      Some("global.guest")
    );
    let init = runtime(
      "/global.guest-123.init.v",
      serde_json::json!({"io.nanocl.v":"global.guest", "io.nanocl.kind":"vm", "io.nanocl.init-v":"true"}),
    );
    assert!(
      super::managed_vm_runtime_key(&init, true)
        .unwrap()
        .is_none()
    );
    let cargo = runtime(
      "/global.web.c",
      serde_json::json!({"io.nanocl.kind":"cargo"}),
    );
    assert!(
      super::managed_vm_runtime_key(&cargo, false)
        .unwrap()
        .is_none()
    );
    assert!(super::managed_vm_runtime_key(&cargo, true).is_err());
    for labels in [
      serde_json::json!({"io.nanocl.kind":"vm"}),
      serde_json::json!({"io.nanocl.v":"guest"}),
      serde_json::json!({"io.nanocl.v":"other.guest"}),
    ] {
      assert!(
        super::managed_vm_runtime_key(
          &runtime("/global.guest.v", labels),
          false
        )
        .is_err()
      );
    }
  }

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
