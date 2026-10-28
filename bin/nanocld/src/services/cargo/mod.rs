use ntex::web;

pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;
pub mod list_history;
pub mod patch;
pub mod put;
pub mod revert;

pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use list::*;
pub use list_history::*;
pub use patch::*;
pub use put::*;
pub use revert::*;

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  config.service(
    web::resource("/cargoes")
      .name("create_cargo")
      .guard(web::guard::Post())
      .to_with_state(create_cargo),
  );
  config.service(
    web::resource("/cargoes/{key}")
      .name("delete_cargo")
      .guard(web::guard::Delete())
      .to_with_state(delete_cargo),
  );
  config.service(
    web::resource("/cargoes/{key}")
      .name("patch_cargo")
      .guard(web::guard::Patch())
      .to_with_state(patch_cargo),
  );
  config.service(
    web::resource("/cargoes/{key}")
      .name("put_cargo")
      .guard(web::guard::Put())
      .to_with_state(put_cargo),
  );
  config.service(
    web::resource("/cargoes")
      .name("list_cargo")
      .guard(web::guard::Get())
      .to_with_state(list_cargo),
  );
  config.service(
    web::resource("/cargoes/{key}/inspect")
      .name("inspect_cargo")
      .guard(web::guard::Get())
      .to_with_state(inspect_cargo),
  );
  config.service(
    web::resource("/cargoes/{key}/histories")
      .name("list_cargo_history")
      .guard(web::guard::Get())
      .to_with_state(list_cargo_history),
  );
  config.service(
    web::resource("/cargoes/{cargo_key}/histories/{history_key}/revert")
      .name("revert_cargo")
      .guard(web::guard::Patch())
      .to_with_state(revert_cargo),
  );
  config.service(
    web::resource("/cargoes/count")
      .name("count_cargo")
      .guard(web::guard::Get())
      .to_with_state(count_cargo),
  );
}

#[cfg(test)]
mod tests {
  use bollard_next::models::HealthConfig;
  use futures::{StreamExt, TryStreamExt};
  use ntex::http;

  use nanocl_stubs::{
    cargo::{Cargo, CargoDeleteQuery, CargoInspect, CargoSummary},
    cargo_spec::{
      CargoSpec, CargoSpecPatch, CargoSpecRevision, Config, ContainerSpec,
    },
    generic::{
      GenericClause, GenericCount, GenericFilter, GenericListQueryNsp,
      GenericNspQuery, ImagePullPolicy,
    },
    namespace::NamespacePartial,
    process::{ProcessKind, ProcessPartial},
    proxy::ProxySslConfig,
    secret::SecretPartial,
    system::{
      Event, EventActorKind, EventCondition, EventKind, NativeEventAction,
      ObjPsHealthStatusKind, ObjPsStatusKind,
    },
  };

  use crate::{
    models::{
      CargoDb, CargoReplicaDb, CargoReplicaProcessDb, CargoReplicaProcessRole,
      DistributedMutexDb, OperationDb, ProcessDb,
    },
    repositories::generic::*,
    utils::{
      container::cargo_compiler::{LABEL_CONTAINER, LABEL_ROLE},
      tests::*,
    },
  };

  const ENDPOINT: &str = "/cargoes";

  fn container(name: &str, image: &str) -> ContainerSpec {
    ContainerSpec {
      name: name.to_owned(),
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: ImagePullPolicy::IfNotPresent,
      container_config: Config {
        image: Some(image.to_owned()),
        ..Default::default()
      },
    }
  }

  fn cargo_spec(name: &str, image: &str) -> CargoSpec {
    CargoSpec {
      name: name.to_owned(),
      containers: vec![container("main", image)],
      ..Default::default()
    }
  }

  fn cargo_event_condition(
    key: &str,
    kind: EventKind,
    action: NativeEventAction,
  ) -> EventCondition {
    EventCondition {
      actor_key: Some(key.to_owned()),
      actor_kind: Some(EventActorKind::Cargo),
      related_key: None,
      related_kind: None,
      kind: vec![kind],
      action: vec![action],
    }
  }

  async fn collect_events(
    stream: crate::models::RawEventReceiver,
  ) -> Vec<Event> {
    ntex::time::timeout(std::time::Duration::from_secs(360), async move {
      let mut stream = stream;
      let mut events = Vec::new();
      while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        if !chunk.is_empty() {
          events.push(serde_json::from_slice(&chunk).unwrap());
        }
      }
      events
    })
    .await
    .expect("timed out waiting for Cargo event")
  }

  fn active_cargo_process<'a>(
    cargo: &'a CargoInspect,
    role: &str,
    name: &str,
  ) -> &'a nanocl_stubs::process::Process {
    cargo
      .instances
      .iter()
      .find(|process| {
        if process.name.ends_with(".tmp.c")
          || process.name.ends_with(".candidate.c")
        {
          return false;
        }
        let labels = process
          .data
          .config
          .as_ref()
          .and_then(|config| config.labels.as_ref());
        labels
          .and_then(|labels| labels.get(LABEL_ROLE))
          .map(String::as_str)
          == Some(role)
          && labels
            .and_then(|labels| labels.get(LABEL_CONTAINER))
            .map(String::as_str)
            == Some(name)
      })
      .unwrap_or_else(|| panic!("missing Cargo {role} process {name}"))
  }

  fn process_started_at(
    process: &nanocl_stubs::process::Process,
  ) -> Option<&str> {
    process
      .data
      .state
      .as_ref()
      .and_then(|state| state.started_at.as_deref())
  }

  async fn cargo_completion_events(
    system: &TestSystem,
    key: &str,
    action: NativeEventAction,
  ) -> crate::models::RawEventReceiver {
    let mut condition = cargo_event_condition(key, EventKind::Normal, action);
    condition.kind.push(EventKind::Error);
    system
      .state
      .subscribe_raw(Some(vec![condition]))
      .await
      .unwrap()
  }

  async fn wait_cargo_completion(
    system: &TestSystem,
    key: &str,
    action: NativeEventAction,
    events: crate::models::RawEventReceiver,
  ) {
    let events = collect_events(events).await;
    let completed =
      cargo_event_condition(key, EventKind::Normal, action.clone());
    let failed = cargo_event_condition(key, EventKind::Error, action.clone());
    assert!(
      events.iter().any(|event| completed == *event),
      "Cargo {key} did not complete {action}: {events:?}"
    );
    assert!(
      !events.iter().any(|event| failed == *event),
      "Cargo {key} failed {action}: {events:?}"
    );
    system
      .state
      .inner
      .task_manager
      .wait_task(&format!("{}@{key}", EventActorKind::Cargo))
      .await;
  }

  async fn start_cargo(system: &TestSystem, key: &str) {
    let events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        key,
        EventKind::Normal,
        NativeEventAction::Start,
      )]))
      .await
      .unwrap();
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{key}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::ACCEPTED,
      "start Cargo update fixture"
    );
    let events = collect_events(events).await;
    assert!(events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Start.to_string()
    }));
    system
      .state
      .inner
      .task_manager
      .wait_task(&format!("{}@{key}", EventActorKind::Cargo))
      .await;
  }

  async fn restart_cargo(system: &TestSystem, key: &str) {
    let events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        key,
        EventKind::Normal,
        NativeEventAction::Restart,
      )]))
      .await
      .unwrap();
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{key}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::ACCEPTED,
      "restart Cargo fixture"
    );
    let events = collect_events(events).await;
    assert!(events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Restart.to_string()
    }));
    system
      .state
      .inner
      .task_manager
      .wait_task(&format!("{}@{key}", EventActorKind::Cargo))
      .await;
  }

  async fn inspect_cargo_fixture(
    system: &TestSystem,
    key: &str,
  ) -> CargoInspect {
    let response = system
      .client
      .send_get(&format!("{ENDPOINT}/{key}/inspect"), None::<String>)
      .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    response.json::<CargoInspect>().await.unwrap()
  }

  async fn wait_cargo_failure(
    system: &TestSystem,
    key: &str,
    action: NativeEventAction,
    events: crate::models::RawEventReceiver,
  ) {
    let events = collect_events(events).await;
    let failed = cargo_event_condition(key, EventKind::Error, action);
    assert!(events.iter().any(|event| failed == *event), "{events:?}");
    system
      .state
      .inner
      .task_manager
      .wait_task(&format!("{}@{key}", EventActorKind::Cargo))
      .await;
    // The registered task waits for the detached executor. Acquiring its
    // lifecycle guard also proves all operation-owned work has released it.
    let guard = tokio::time::timeout(
      std::time::Duration::from_secs(30),
      crate::utils::container::cargo::lifecycle_lock(key).lock_owned(),
    )
    .await
    .expect("Cargo executor did not release its lifecycle guard");
    drop(guard);
  }

  async fn delete_cargo_fixture(system: &TestSystem, key: &str) {
    let events =
      cargo_completion_events(system, key, NativeEventAction::Destroy).await;
    let response = system
      .client
      .send_delete(
        &format!("{ENDPOINT}/{key}"),
        Some(CargoDeleteQuery { force: Some(true) }),
      )
      .await;
    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    wait_cargo_completion(system, key, NativeEventAction::Destroy, events)
      .await;
  }

  fn operation_fixture_spec(name: &str, init_command: &str) -> CargoSpec {
    let mut spec = cargo_spec(name, "alpine:latest");
    spec.containers[0].container_config.cmd =
      Some(vec!["sleep".to_owned(), "3600".to_owned()]);
    let mut init = container("prepare", "alpine:latest");
    init.container_config.cmd = Some(vec![
      "sh".to_owned(),
      "-c".to_owned(),
      init_command.to_owned(),
    ]);
    spec.init_containers.push(init);
    spec
  }

  async fn cargo_runtime_snapshot(
    system: &TestSystem,
    key: &str,
  ) -> std::collections::BTreeMap<String, serde_json::Value> {
    use bollard_next::query_parameters::ListContainersOptions;
    let containers = system
      .state
      .inner
      .docker_api
      .list_containers(Some(ListContainersOptions {
        all: true,
        filters: Some(std::collections::HashMap::from([(
          "label".to_owned(),
          vec![format!("io.nanocl.c={key}")],
        )])),
        ..Default::default()
      }))
      .await
      .unwrap();
    let mut result = std::collections::BTreeMap::new();
    for container in containers {
      let id = container.id.expect("Docker inventory must contain IDs");
      let runtime = system
        .state
        .inner
        .docker_api
        .inspect_container(&id, None)
        .await
        .unwrap();
      let state = runtime.state.unwrap();
      result.insert(
        id,
        serde_json::json!({
          "StartedAt": state.started_at,
          "Running": state.running,
          "ExitCode": state.exit_code,
          "RestartCount": runtime.restart_count,
          "Labels": runtime.config.and_then(|config| config.labels),
        }),
      );
    }
    result
  }

  fn assert_no_application(cargo: &CargoInspect) {
    assert!(!cargo.instances.iter().any(|process| {
      process
        .data
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref())
        .and_then(|labels| labels.get(LABEL_ROLE))
        .is_some_and(|role| role == "app")
    }));
  }

  #[ntex::test]
  async fn confirmed_init_failure_settles_and_allows_delete() {
    let system = gen_default_test_system().await;
    let name = format!("init-exit-{}", uuid::Uuid::new_v4().simple());
    let key = format!("global.{name}");
    let spec =
      operation_fixture_spec(&name, "echo sensitive-init-output; exit 7");
    let response = system
      .client
      .send_post(ENDPOINT, Some(&spec), None::<String>)
      .await;
    assert_eq!(response.status(), http::StatusCode::CREATED);
    let events =
      cargo_completion_events(&system, &key, NativeEventAction::Start).await;
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{key}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    wait_cargo_failure(&system, &key, NativeEventAction::Start, events).await;

    let failed = OperationDb::current("Cargo", &key, &system.state.inner.pool)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(failed.action, "start");
    assert_eq!(failed.outcome.as_deref(), Some("failed"));
    assert!(failed.completed_at.is_some());
    assert_eq!(
      failed.diagnostic,
      Some(serde_json::json!({
        "Phase": "start", "Failure": "Init container exited with code 7"
      }))
    );
    let inspected = inspect_cargo_fixture(&system, &key).await;
    assert_eq!(inspected.status.actual, ObjPsStatusKind::Fail);
    assert_no_application(&inspected);
    let init = active_cargo_process(&inspected, "init", "prepare");
    let runtime = cargo_runtime_snapshot(&system, &key).await;
    assert_eq!(runtime[&init.key]["ExitCode"], 7);
    assert!(
      !runtime
        .values()
        .any(|runtime| runtime["Labels"][LABEL_ROLE] == "app")
    );
    assert!(
      DistributedMutexDb::read_by_pk(
        &failed.key.to_string(),
        &system.state.inner.pool
      )
      .await
      .is_err()
    );

    delete_cargo_fixture(&system, &key).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn uncertain_init_wait_stays_pending_until_observed_recovery() {
    use crate::utils::container::cargo::{
      fail_next_init_wait, recovery::recover_one,
    };
    use bollard_next::{
      errors::Error as DockerError, query_parameters::WaitContainerOptions,
    };

    let system = gen_default_test_system().await;
    let name = format!("init-wait-{}", uuid::Uuid::new_v4().simple());
    let key = format!("global.{name}");
    let spec = operation_fixture_spec(&name, "sleep 1; exit 7");
    let response = system
      .client
      .send_post(ENDPOINT, Some(&spec), None::<String>)
      .await;
    assert_eq!(response.status(), http::StatusCode::CREATED);
    let _fault = fail_next_init_wait(&key);
    let events =
      cargo_completion_events(&system, &key, NativeEventAction::Start).await;
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{key}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    wait_cargo_failure(&system, &key, NativeEventAction::Start, events).await;
    let pending = OperationDb::current("Cargo", &key, &system.state.inner.pool)
      .await
      .unwrap()
      .unwrap();
    assert!(pending.outcome.is_none());
    assert!(pending.completed_at.is_none());
    let inspected = inspect_cargo_fixture(&system, &key).await;
    assert_no_application(&inspected);
    let init = active_cargo_process(&inspected, "init", "prepare");
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{key}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(response.status(), http::StatusCode::CONFLICT);
    let observed_exit = tokio::time::timeout(
      std::time::Duration::from_secs(30),
      system
        .state
        .inner
        .docker_api
        .wait_container(
          &init.key,
          Some(WaitContainerOptions {
            condition: "not-running".to_owned(),
          }),
        )
        .next(),
    )
    .await
    .unwrap();
    assert!(matches!(
      observed_exit,
      Some(Err(DockerError::DockerContainerWaitError { code: 7, .. }))
    ));
    let before = cargo_runtime_snapshot(&system, &key).await;
    let cargo = CargoDb::transform_read_by_pk(&key, &system.state.inner.pool)
      .await
      .unwrap();
    let observed =
      crate::utils::container::cargo::observe(&cargo, &system.state)
        .await
        .unwrap();
    assert!(recover_one(&pending, &system.state).await.unwrap());
    let recovered = OperationDb::get(pending.key, &system.state.inner.pool)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(recovered.outcome.as_deref(), Some("interrupted"));
    assert_eq!(
      inspect_cargo_fixture(&system, &key)
        .await
        .status
        .actual
        .to_string(),
      observed.actual.unwrap()
    );
    assert_eq!(cargo_runtime_snapshot(&system, &key).await, before);
    delete_cargo_fixture(&system, &key).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn restart_completion_write_failure_recovers_without_replay() {
    use crate::{
      repositories::cargo_operation::completion_fault::fail_next_completion,
      utils::{
        container::cargo::recovery::recover_one, operation::abandoned_owner,
      },
    };

    let system = gen_default_test_system().await;
    let name = format!("restart-finish-{}", uuid::Uuid::new_v4().simple());
    let key = format!("global.{name}");
    let spec = operation_fixture_spec(&name, "true");
    let response = system
      .client
      .send_post(ENDPOINT, Some(&spec), None::<String>)
      .await;
    assert_eq!(response.status(), http::StatusCode::CREATED);
    start_cargo(&system, &key).await;
    let before = cargo_runtime_snapshot(&system, &key).await;
    let original = inspect_cargo_fixture(&system, &key).await;
    let app_id = active_cargo_process(&original, "app", "main").key.clone();
    let fault = fail_next_completion(&key, "restart");
    let events =
      cargo_completion_events(&system, &key, NativeEventAction::Restart).await;
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{key}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    wait_cargo_failure(&system, &key, NativeEventAction::Restart, events).await;
    assert!(fault.was_triggered());
    let pending = OperationDb::current("Cargo", &key, &system.state.inner.pool)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(pending.action, "restart");
    assert!(pending.outcome.is_none());
    assert!(pending.completed_at.is_none());
    assert!(
      DistributedMutexDb::read_by_pk(
        &pending.key.to_string(),
        &system.state.inner.pool
      )
      .await
      .is_ok()
    );
    let restarted = cargo_runtime_snapshot(&system, &key).await;
    assert_eq!(
      before.keys().collect::<Vec<_>>(),
      restarted.keys().collect::<Vec<_>>()
    );
    assert_ne!(
      before[&app_id]["StartedAt"],
      restarted[&app_id]["StartedAt"]
    );
    assert_eq!(restarted[&app_id]["Running"], true);
    assert_eq!(
      inspect_cargo_fixture(&system, &key).await.status.actual,
      ObjPsStatusKind::Start
    );

    // A real file owner prevents recovery even when the task has ended.
    let owner = abandoned_owner(
      &pending.owner(),
      &system.state.inner.config.hostname,
      &system.state.inner.config.state_dir,
    )
    .unwrap()
    .expect("completed executor retained its operation file lock");
    assert!(!recover_one(&pending, &system.state).await.unwrap());
    drop(owner);
    let mut stale = pending.clone();
    stale.key = uuid::Uuid::new_v4();
    assert!(!recover_one(&stale, &system.state).await.unwrap());
    stale = pending.clone();
    stale.incarnation_key = uuid::Uuid::new_v4();
    assert!(!recover_one(&stale, &system.state).await.unwrap());

    // Incomplete attachment evidence must not settle a healthy-looking runtime.
    let replica = CargoReplicaDb::list_by_cargo(&key, &system.state.inner.pool)
      .await
      .unwrap()
      .remove(0);
    let mapping = CargoReplicaProcessDb::list_by_replica(
      replica.key,
      &system.state.inner.pool,
    )
    .await
    .unwrap()
    .into_iter()
    .find(|mapping| mapping.process_key.as_deref() == Some(app_id.as_str()))
    .unwrap();
    CargoReplicaProcessDb::clear_process(mapping.key, &system.state.inner.pool)
      .await
      .unwrap();
    let recovery = recover_one(&pending, &system.state).await;
    CargoReplicaProcessDb::attach_process(
      mapping.key,
      &app_id,
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    assert!(recovery.is_err());
    assert!(
      OperationDb::get(pending.key, &system.state.inner.pool)
        .await
        .unwrap()
        .unwrap()
        .outcome
        .is_none()
    );
    assert_eq!(cargo_runtime_snapshot(&system, &key).await, restarted);

    assert!(recover_one(&pending, &system.state).await.unwrap());
    let recovered = OperationDb::get(pending.key, &system.state.inner.pool)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(recovered.outcome.as_deref(), Some("interrupted"));
    assert!(recovered.completed_at.is_some());
    assert_eq!(recovered.incarnation_key, pending.incarnation_key);
    assert_eq!(recovered.spec_key, pending.spec_key);
    assert_eq!(cargo_runtime_snapshot(&system, &key).await, restarted);
    assert_eq!(
      inspect_cargo_fixture(&system, &key).await.status.actual,
      ObjPsStatusKind::Start
    );
    assert!(
      DistributedMutexDb::read_by_pk(
        &pending.key.to_string(),
        &system.state.inner.pool
      )
      .await
      .is_err()
    );

    restart_cargo(&system, &key).await;
    let successor =
      OperationDb::current("Cargo", &key, &system.state.inner.pool)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(successor.key, pending.key);
    assert_eq!(successor.outcome.as_deref(), Some("completed"));
    assert!(!recover_one(&pending, &system.state).await.unwrap());
    delete_cargo_fixture(&system, &key).await;
    let response = system
      .client
      .send_post(ENDPOINT, Some(&spec), None::<String>)
      .await;
    assert_eq!(response.status(), http::StatusCode::CREATED);
    let (recreated, _, _) = CargoDb::read_by_pk(&key, &system.state.inner.pool)
      .await
      .unwrap();
    assert_ne!(recreated.incarnation_key, pending.incarnation_key);
    assert!(!recover_one(&pending, &system.state).await.unwrap());
    delete_cargo_fixture(&system, &key).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn collection_namespace_and_key_semantics() {
    const NAME: &str = "same-name-namespace-test";
    const NAMESPACE: &str = "cargo-collection-test";
    const GLOBAL_KEY: &str = "global.same-name-namespace-test";
    const OTHER_KEY: &str = "cargo-collection-test.same-name-namespace-test";

    let system = gen_default_test_system().await;
    let client = &system.client;
    let response = client
      .send_post(
        "/namespaces",
        Some(NamespacePartial {
          name: NAMESPACE.to_owned(),
          metadata: None,
        }),
        None::<String>,
      )
      .await;
    assert!(
      response.status().is_success()
        || response.status() == http::StatusCode::CONFLICT
    );

    let spec = cargo_spec(NAME, "alpine:latest");
    for namespace in [None, Some(NAMESPACE)] {
      let response = client
        .send_post(ENDPOINT, Some(&spec), Some(GenericNspQuery::new(namespace)))
        .await;
      assert!(
        response.status().is_success()
          || response.status() == http::StatusCode::CONFLICT
      );
    }

    let filter =
      GenericFilter::new().r#where("name", GenericClause::Eq(NAME.to_owned()));
    let query = |namespace: Option<&str>| {
      let mut query = GenericListQueryNsp::try_from(filter.clone()).unwrap();
      query.namespace = namespace.map(str::to_owned);
      query
    };

    for namespace in [None, Some(""), Some(" \t ")] {
      let response = client.send_get(ENDPOINT, Some(query(namespace))).await;
      test_status_code!(
        response.status(),
        http::StatusCode::OK,
        "unscoped cargo list"
      );
      let cargoes = response.json::<Vec<CargoSummary>>().await.unwrap();
      let keys = cargoes
        .iter()
        .map(|cargo| cargo.spec.cargo_key.as_str())
        .collect::<Vec<_>>();
      assert!(keys.contains(&GLOBAL_KEY));
      assert!(keys.contains(&OTHER_KEY));

      let response = client
        .send_get("/cargoes/count", Some(query(namespace)))
        .await;
      let count = response.json::<GenericCount>().await.unwrap();
      assert_eq!(count.count as usize, cargoes.len());
    }

    for (namespace, expected_key) in
      [("global", GLOBAL_KEY), (NAMESPACE, OTHER_KEY)]
    {
      let response = client
        .send_get(ENDPOINT, Some(query(Some(namespace))))
        .await;
      let cargoes = response.json::<Vec<CargoSummary>>().await.unwrap();
      assert_eq!(cargoes.len(), 1);
      assert_eq!(cargoes[0].spec.cargo_key, expected_key);

      let response = client
        .send_get("/cargoes/count", Some(query(Some(namespace))))
        .await;
      let count = response.json::<GenericCount>().await.unwrap();
      assert_eq!(count.count, 1);
    }

    let response = client
      .send_get(&format!("{ENDPOINT}/{OTHER_KEY}/inspect"), None::<String>)
      .await;
    let cargo = response.json::<CargoInspect>().await.unwrap();
    assert_eq!(cargo.namespace_name, NAMESPACE);

    let response = client
      .send_get(&format!("{ENDPOINT}/not-a-key/inspect"), None::<String>)
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::BAD_REQUEST,
      "invalid cargo key"
    );

    let response = client
      .send_patch(
        &format!("{ENDPOINT}/{OTHER_KEY}"),
        Some(CargoSpecPatch {
          name: Some("renamed".to_owned()),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::BAD_REQUEST,
      "cargo name is immutable"
    );

    let response = client
      .send_patch(
        &format!("{ENDPOINT}/{OTHER_KEY}"),
        Some(CargoSpecPatch {
          containers: Some(vec![container("main", "busybox:latest")]),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::OK,
      "patch cargo by canonical key"
    );
    // This test exercises definition/key semantics on a stopped Cargo. The
    // accepted update may fail because no runtime was started; it must finish
    // before the independent foreign-history validation is submitted.
    system
      .state
      .inner
      .task_manager
      .wait_task(&format!("{}@{OTHER_KEY}", EventActorKind::Cargo))
      .await;
    let operation =
      OperationDb::current("Cargo", OTHER_KEY, &system.state.inner.pool)
        .await
        .unwrap()
        .expect("accepted Cargo update operation");
    assert_eq!(operation.action, "update");
    assert!(
      operation.outcome.is_some(),
      "Cargo update must settle before revert validation"
    );

    let response = client
      .send_get(
        &format!("{ENDPOINT}/{GLOBAL_KEY}/histories"),
        None::<String>,
      )
      .await;
    let global_history = response
      .json::<Vec<CargoSpecRevision>>()
      .await
      .unwrap()
      .into_iter()
      .next()
      .unwrap();
    let response = client
      .send_patch(
        &format!(
          "{ENDPOINT}/{OTHER_KEY}/histories/{}/revert",
          global_history.key
        ),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::BAD_REQUEST,
      "reject another Cargo's history even when its name matches"
    );

    let response = client
      .send_patch(
        &format!(
          "{ENDPOINT}/{OTHER_KEY}/histories/{}/revert",
          uuid::Uuid::new_v4()
        ),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::NOT_FOUND,
      "missing Cargo history remains not found"
    );

    for key in [GLOBAL_KEY, OTHER_KEY] {
      let events =
        cargo_completion_events(&system, key, NativeEventAction::Destroy).await;
      let response = client
        .send_delete(
          &format!("{ENDPOINT}/{key}"),
          Some(CargoDeleteQuery { force: Some(true) }),
        )
        .await;
      test_status_code!(
        response.status(),
        http::StatusCode::ACCEPTED,
        "delete cargo by canonical key"
      );
      wait_cargo_completion(&system, key, NativeEventAction::Destroy, events)
        .await;
    }
  }

  /// Test to create start patch stop and delete a cargo with valid data
  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = &system.client;
    let test_cargoes = [
      "1daemon-test-cargo",
      "2another-test-cargo",
      "2daemon-test-cargo",
    ];
    let main_test_cargo = test_cargoes[0];
    let main_test_key = format!("global.{main_test_cargo}");
    for test_cargo in test_cargoes.iter() {
      let test_cargo = test_cargo.to_owned();
      let expected =
        cargo_spec(test_cargo, "ghcr.io/next-hat/nanocl-get-started:latest");
      let res = client
        .send_post(ENDPOINT, Some(&expected), None::<String>)
        .await;
      test_status_code!(
        res.status(),
        http::StatusCode::CREATED,
        "basic cargo create"
      );
      let cargo = TestClient::res_json::<Cargo>(res).await;
      assert_eq!(cargo.spec.name, test_cargo, "Invalid cargo name");
      assert_eq!(cargo.namespace_name, "global", "Invalid cargo namespace");
      assert_eq!(cargo.spec.containers, expected.containers);
    }
    let res = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_key}/inspect"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo inspect"
    );
    let response = res.json::<CargoInspect>().await.unwrap();
    assert_eq!(
      response.spec.name, main_test_cargo,
      "Expected to find cargo with name {main_test_cargo} got {}",
      response.spec.name
    );
    let res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo list");
    let cargoes = res.json::<Vec<CargoSummary>>().await.unwrap();
    assert!(!cargoes.is_empty(), "Expected to find cargoes");
    start_cargo(&system, &main_test_key).await;
    let res = client
      .send_get(
        &format!("/processes/cargo/{main_test_key}/logs"),
        Some(nanocl_stubs::process::ProcessLogQuery {
          follow: Some(false),
          tail: Some("1".to_owned()),
          ..Default::default()
        }),
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo logs by canonical key"
    );
    let before_restart = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_key}/inspect"),
        None::<String>,
      )
      .await
      .json::<CargoInspect>()
      .await
      .unwrap();
    let before_app = active_cargo_process(&before_restart, "app", "main");
    assert_eq!(
      before_restart.instances.len(),
      1,
      "single-application Cargo must have only its direct application process"
    );
    let before_app_id = before_app.key.clone();
    let before_app_started = process_started_at(before_app).map(str::to_owned);
    let replicas =
      CargoReplicaDb::list_by_cargo(&main_test_key, &system.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(replicas.len(), 1);
    let before_mappings = CargoReplicaProcessDb::list_by_replica(
      replicas[0].key,
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    assert_eq!(
      before_mappings.len(),
      1,
      "single-application Cargo must not persist a sandbox mapping"
    );
    assert_eq!(before_mappings[0].role, CargoReplicaProcessRole::App);
    assert_eq!(before_mappings[0].container_name, "main");
    assert_eq!(
      before_mappings[0].process_key.as_deref(),
      Some(before_app_id.as_str())
    );
    restart_cargo(&system, &main_test_key).await;
    let after_restart = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_key}/inspect"),
        None::<String>,
      )
      .await
      .json::<CargoInspect>()
      .await
      .unwrap();
    assert_eq!(after_restart.status.actual, ObjPsStatusKind::Start);
    let after_app = active_cargo_process(&after_restart, "app", "main");
    assert_eq!(
      after_restart.instances.len(),
      1,
      "single-application Cargo must remain direct after restart"
    );
    assert_eq!(after_app.key, before_app_id);
    assert_ne!(
      process_started_at(after_app),
      before_app_started.as_deref(),
      "one healthless application must be Docker-restarted"
    );
    let after_mappings = CargoReplicaProcessDb::list_by_replica(
      replicas[0].key,
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    assert_eq!(
      after_mappings.len(),
      1,
      "restart must not add a sandbox mapping to a single-application Cargo"
    );
    assert_eq!(after_mappings[0].key, before_mappings[0].key);
    assert_eq!(after_mappings[0].role, CargoReplicaProcessRole::App);
    assert_eq!(after_mappings[0].container_name, "main");
    assert_eq!(
      after_mappings[0].process_key.as_deref(),
      Some(after_app.key.as_str())
    );
    let mut replacement_container =
      container("main", "ghcr.io/next-hat/nanocl-get-started:latest");
    replacement_container.container_config.env =
      Some(vec!["TEST=1".to_owned()]);
    let replacement = CargoSpec {
      name: main_test_cargo.to_owned(),
      containers: vec![replacement_container],
      ..Default::default()
    };
    let events = cargo_completion_events(
      &system,
      &main_test_key,
      NativeEventAction::Update,
    )
    .await;
    let res = client
      .send_put(
        &format!("{ENDPOINT}/{main_test_key}"),
        Some(&replacement),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo patch");
    let patch_response = res.json::<Cargo>().await.unwrap();
    assert_eq!(patch_response.spec.name, main_test_cargo);
    assert_eq!(patch_response.namespace_name, "global");
    assert_eq!(patch_response.spec.containers, replacement.containers);
    wait_cargo_completion(
      &system,
      &main_test_key,
      NativeEventAction::Update,
      events,
    )
    .await;
    let res = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_key}/histories"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo history"
    );
    let histories = res.json::<Vec<CargoSpecRevision>>().await.unwrap();
    assert!(histories.len() > 1, "Expected to find cargo histories");
    let history = histories
      .iter()
      .find(|history| history.containers != replacement.containers)
      .expect("Expected to find the original Cargo declaration")
      .clone();
    let id = history.key;
    let events = cargo_completion_events(
      &system,
      &main_test_key,
      NativeEventAction::Update,
    )
    .await;
    let res = client
      .send_patch(
        &format!("{ENDPOINT}/{main_test_key}/histories/{id}/revert"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo revert");
    let reverted = res.json::<Cargo>().await.unwrap();
    assert_eq!(reverted.spec.name, history.name);
    assert_eq!(reverted.spec.metadata, history.metadata);
    assert_eq!(reverted.spec.replicas, history.replicas);
    assert_eq!(reverted.spec.network_mode, history.network_mode);
    assert_eq!(reverted.spec.port_bindings, history.port_bindings);
    assert_eq!(reverted.spec.secrets, history.secrets);
    assert_eq!(reverted.spec.placement, history.placement);
    assert_eq!(
      reverted.spec.resource_requirement,
      history.resource_requirement
    );
    assert_eq!(reverted.spec.init_containers, history.init_containers);
    assert_eq!(reverted.spec.containers, history.containers);
    wait_cargo_completion(
      &system,
      &main_test_key,
      NativeEventAction::Update,
      events,
    )
    .await;
    let events =
      cargo_completion_events(&system, &main_test_key, NativeEventAction::Stop)
        .await;
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_key}/stop"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo stop"
    );
    wait_cargo_completion(
      &system,
      &main_test_key,
      NativeEventAction::Stop,
      events,
    )
    .await;
    for test_cargo in test_cargoes.iter() {
      let key = format!("global.{test_cargo}");
      let events =
        cargo_completion_events(&system, &key, NativeEventAction::Destroy)
          .await;
      let res = client
        .send_delete(
          &format!("{ENDPOINT}/global.{test_cargo}"),
          Some(CargoDeleteQuery { force: Some(true) }),
        )
        .await;
      test_status_code!(
        res.status(),
        http::StatusCode::ACCEPTED,
        "basic cargo delete"
      );
      wait_cargo_completion(&system, &key, NativeEventAction::Destroy, events)
        .await;
    }
    // test init container
    let mut init = container("init", "alpine:latest");
    init.container_config.cmd =
      Some(vec!["echo".to_owned(), "hello".to_owned()]);
    let init_spec = CargoSpec {
      name: "init-test-cargo".to_owned(),
      init_containers: vec![init],
      containers: vec![container(
        "main",
        "ghcr.io/next-hat/nanocl-get-started:latest",
      )],
      ..Default::default()
    };
    let res = client
      .send_post(ENDPOINT, Some(&init_spec), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::CREATED,
      "init cargo create"
    );
    let cargo = TestClient::res_json::<Cargo>(res).await;
    assert_eq!(cargo.spec.name, "init-test-cargo", "Invalid cargo name");
    start_cargo(&system, "global.init-test-cargo").await;
    let events = cargo_completion_events(
      &system,
      "global.init-test-cargo",
      NativeEventAction::Destroy,
    )
    .await;
    let res = client
      .send_delete(
        &format!("{ENDPOINT}/global.init-test-cargo"),
        Some(CargoDeleteQuery { force: Some(true) }),
      )
      .await;
    assert_eq!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "init cargo delete"
    );
    wait_cargo_completion(
      &system,
      "global.init-test-cargo",
      NativeEventAction::Destroy,
      events,
    )
    .await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn multi_container_restart_preserves_sandbox_and_completed_init() {
    const NAME: &str = "multi-container-restart-readiness";
    const KEY: &str = "global.multi-container-restart-readiness";

    let system = gen_default_test_system().await;
    let mut prepare = container("prepare", "alpine:latest");
    prepare.container_config.cmd =
      Some(vec!["sh".to_owned(), "-c".to_owned(), "true".to_owned()]);

    let delayed_healthcheck = |name: &str, delay: u64| {
      let mut app = container(name, "alpine:latest");
      app.container_config.cmd = Some(vec![
        "sh".to_owned(),
        "-c".to_owned(),
        format!("sleep {delay}; exec sleep 3600"),
      ]);
      app.container_config.healthcheck = Some(HealthConfig {
        test: Some(vec![
          "CMD-SHELL".to_owned(),
          "test \"$(cat /proc/1/comm)\" = sleep".to_owned(),
        ]),
        interval: Some(500_000_000),
        timeout: Some(1_000_000_000),
        retries: Some(5),
        start_period: Some(10_000_000_000),
        ..Default::default()
      });
      app
    };
    let healthless = |name: &str| {
      let mut app = container(name, "alpine:latest");
      app.container_config.cmd = Some(vec![
        "sh".to_owned(),
        "-c".to_owned(),
        "while true; do sleep 3600; done".to_owned(),
      ]);
      app
    };
    let mut metrics = healthless("metrics");
    metrics.essential = false;
    metrics.container_config.healthcheck = Some(HealthConfig {
      test: Some(vec!["CMD-SHELL".to_owned(), "false".to_owned()]),
      interval: Some(500_000_000),
      timeout: Some(1_000_000_000),
      retries: Some(1),
      ..Default::default()
    });
    let spec = CargoSpec {
      name: NAME.to_owned(),
      init_containers: vec![prepare],
      containers: vec![
        // Restart the nonessential probe before the delayed essentials so its
        // unhealthy observation cannot race the successful readiness point.
        metrics,
        healthless("worker-a"),
        delayed_healthcheck("api", 1),
        healthless("worker-b"),
        delayed_healthcheck("queue", 3),
      ],
      ..Default::default()
    };
    let response = system
      .client
      .send_post(ENDPOINT, Some(&spec), None::<String>)
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::CREATED,
      "create Cargo restart fixture"
    );
    start_cargo(&system, KEY).await;

    let before = system
      .client
      .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
      .await
      .json::<CargoInspect>()
      .await
      .unwrap();
    let sandbox = active_cargo_process(&before, "sandbox", "_sandbox");
    let init = active_cargo_process(&before, "init", "prepare");
    let sandbox_identity = (
      sandbox.key.clone(),
      process_started_at(sandbox).map(str::to_owned),
    );
    let init_identity = (
      init.key.clone(),
      process_started_at(init).map(str::to_owned),
      init
        .data
        .state
        .as_ref()
        .and_then(|state| state.finished_at.clone()),
    );
    let before_apps = spec
      .containers
      .iter()
      .map(|declared| {
        let process = active_cargo_process(&before, "app", &declared.name);
        (
          declared.name.clone(),
          (
            process.key.clone(),
            process_started_at(process).map(str::to_owned),
          ),
        )
      })
      .collect::<std::collections::HashMap<_, _>>();

    let mut start_events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        KEY,
        EventKind::Normal,
        NativeEventAction::Start,
      )]))
      .await
      .unwrap();
    let restart_events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        KEY,
        EventKind::Normal,
        NativeEventAction::Restart,
      )]))
      .await
      .unwrap();
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{KEY}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::ACCEPTED,
      "restart final multi-container Cargo"
    );
    let during_restart =
      ntex::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
          let cargo = system
            .client
            .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
            .await
            .json::<CargoInspect>()
            .await
            .unwrap();
          if cargo.status.actual == ObjPsStatusKind::Starting {
            break cargo;
          }
          assert_ne!(cargo.status.actual, ObjPsStatusKind::Fail);
          ntex::time::sleep(std::time::Duration::from_millis(50)).await;
        }
      })
      .await
      .expect("Cargo restart never entered its readiness transition");
    assert_eq!(during_restart.status.health, ObjPsHealthStatusKind::Unknown);
    let premature_start =
      ntex::time::timeout(std::time::Duration::from_millis(500), async {
        while let Some(chunk) = start_events.next().await {
          let chunk = chunk.unwrap();
          if chunk.is_empty() {
            continue;
          }
          let event = serde_json::from_slice::<Event>(&chunk).unwrap();
          if event.kind == EventKind::Normal
            && event.action == NativeEventAction::Start.to_string()
            && event.actor.as_ref().and_then(|actor| actor.key.as_deref())
              == Some(KEY)
          {
            return true;
          }
        }
        false
      })
      .await;
    assert!(
      premature_start.is_err(),
      "restart published Start before every essential app was ready"
    );
    let start_events = collect_events(start_events).await;
    let restart_events = collect_events(restart_events).await;
    assert!(start_events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Start.to_string()
    }));
    assert!(restart_events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Restart.to_string()
    }));

    let after = system
      .client
      .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
      .await
      .json::<CargoInspect>()
      .await
      .unwrap();
    assert_eq!(after.status.actual, ObjPsStatusKind::Start);
    assert_eq!(after.status.health, ObjPsHealthStatusKind::Healthy);
    let sandbox = active_cargo_process(&after, "sandbox", "_sandbox");
    assert_eq!(
      (
        sandbox.key.clone(),
        process_started_at(sandbox).map(str::to_owned)
      ),
      sandbox_identity,
      "sandbox ID and start time must not change"
    );
    let init = active_cargo_process(&after, "init", "prepare");
    assert_eq!(
      (
        init.key.clone(),
        process_started_at(init).map(str::to_owned),
        init
          .data
          .state
          .as_ref()
          .and_then(|state| state.finished_at.clone())
      ),
      init_identity,
      "completed init ID and timestamps must not change"
    );
    for declared in &spec.containers {
      let process = active_cargo_process(&after, "app", &declared.name);
      let (before_key, before_started) = &before_apps[&declared.name];
      assert_eq!(&process.key, before_key);
      assert_ne!(process_started_at(process), before_started.as_deref());
    }
    for name in ["api", "queue"] {
      let process = active_cargo_process(&after, "app", name);
      assert_eq!(
        process
          .data
          .state
          .as_ref()
          .and_then(|state| state.health.as_ref())
          .and_then(|health| health.status.as_ref())
          .map(ToString::to_string)
          .as_deref(),
        Some("healthy"),
        "both essential healthchecked apps must be healthy at success"
      );
    }
    let metrics = active_cargo_process(&after, "app", "metrics");
    assert_eq!(
      metrics
        .data
        .state
        .as_ref()
        .and_then(|state| state.health.as_ref())
        .and_then(|health| health.status.as_ref())
        .map(ToString::to_string)
        .as_deref(),
      Some("unhealthy"),
      "an unhealthy nonessential app must not block restart completion"
    );

    let response = system
      .client
      .send_delete(
        &format!("{ENDPOINT}/{KEY}"),
        Some(CargoDeleteQuery { force: Some(true) }),
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::ACCEPTED,
      "delete Cargo restart fixture"
    );
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn cargo_restart_rejects_missing_sandbox_and_ambiguous_app() {
    const NAME: &str = "invalid-restart-topology";
    const KEY: &str = "global.invalid-restart-topology";

    let system = gen_default_test_system().await;
    let mut app = container("api", "alpine:latest");
    app.container_config.cmd = Some(vec![
      "sh".to_owned(),
      "-c".to_owned(),
      "while true; do sleep 3600; done".to_owned(),
    ]);
    let mut worker = container("worker", "alpine:latest");
    worker.container_config.cmd = Some(vec![
      "sh".to_owned(),
      "-c".to_owned(),
      "while true; do sleep 3600; done".to_owned(),
    ]);
    let response = system
      .client
      .send_post(
        ENDPOINT,
        Some(CargoSpec {
          name: NAME.to_owned(),
          containers: vec![app, worker],
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::CREATED,
      "create invalid restart topology fixture"
    );
    start_cargo(&system, KEY).await;
    let before = system
      .client
      .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
      .await
      .json::<CargoInspect>()
      .await
      .unwrap();
    let app = active_cargo_process(&before, "app", "api");
    let sandbox = active_cargo_process(&before, "sandbox", "_sandbox");
    let app_started = process_started_at(app).map(str::to_owned);
    let sandbox_id = sandbox.key.clone();
    let replicas = CargoReplicaDb::list_by_cargo(KEY, &system.state.inner.pool)
      .await
      .unwrap();
    assert_eq!(replicas.len(), 1);
    let sandbox_mapping = CargoReplicaProcessDb::find(
      replicas[0].key,
      CargoReplicaProcessRole::Sandbox,
      "_sandbox",
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    CargoReplicaProcessDb::set_processes(
      vec![(sandbox_mapping.key, None, true)],
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    let error_events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        KEY,
        EventKind::Error,
        NativeEventAction::Restart,
      )]))
      .await
      .unwrap();
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{KEY}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    let events = collect_events(error_events).await;
    assert!(events.iter().any(|event| {
      event.kind == EventKind::Error
        && event.action == NativeEventAction::Restart.to_string()
    }));
    assert!(
      system
        .state
        .inner
        .docker_api
        .inspect_container(
          &sandbox_id,
          None::<bollard_next::query_parameters::InspectContainerOptions>,
        )
        .await
        .is_ok(),
      "missing sandbox mapping must not recreate or replace Docker sandbox"
    );
    CargoReplicaProcessDb::set_processes(
      vec![(
        sandbox_mapping.key,
        Some(sandbox_id.clone()),
        sandbox_mapping.essential,
      )],
      &system.state.inner.pool,
    )
    .await
    .unwrap();

    let duplicate_key = uuid::Uuid::new_v4().to_string();
    ProcessDb::create_from(
      &ProcessPartial {
        key: duplicate_key.clone(),
        name: format!("{KEY}-duplicate-api.c"),
        kind: ProcessKind::Cargo,
        data: serde_json::to_value(&app.data).unwrap(),
        node_name: system.state.inner.config.hostname.clone(),
        kind_key: KEY.to_owned(),
        created_at: Some(chrono::Utc::now().naive_utc()),
      },
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    let error_events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        KEY,
        EventKind::Error,
        NativeEventAction::Restart,
      )]))
      .await
      .unwrap();
    let response = system
      .client
      .send_post(
        &format!("/processes/cargo/{KEY}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    let events = collect_events(error_events).await;
    assert!(events.iter().any(|event| {
      event.kind == EventKind::Error
        && event.action == NativeEventAction::Restart.to_string()
    }));
    let after = system
      .client
      .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
      .await
      .json::<CargoInspect>()
      .await
      .unwrap();
    assert_eq!(
      process_started_at(active_cargo_process(&after, "app", "api")),
      app_started.as_deref(),
      "topology failure must occur before any Docker app restart"
    );
    ProcessDb::del_by_pk(&duplicate_key, &system.state.inner.pool)
      .await
      .unwrap();

    let response = system
      .client
      .send_delete(
        &format!("{ENDPOINT}/{KEY}"),
        Some(CargoDeleteQuery { force: Some(true) }),
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::ACCEPTED,
      "delete invalid restart topology fixture"
    );
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn multi_container_update_waits_for_complete_candidate_readiness() {
    const NAME: &str = "multi-container-update-readiness";
    const KEY: &str = "global.multi-container-update-readiness";

    let system = gen_default_test_system().await;
    let response = system
      .client
      .send_post(
        ENDPOINT,
        Some(cargo_spec(
          NAME,
          "ghcr.io/next-hat/nanocl-get-started:latest",
        )),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::CREATED,
      "create multi-container update fixture"
    );
    start_cargo(&system, KEY).await;

    let mut prepare_schema = container("prepare-schema", "alpine:latest");
    prepare_schema.container_config.cmd =
      Some(vec!["sh".to_owned(), "-c".to_owned(), "true".to_owned()]);
    let mut prepare_data = container("prepare-data", "alpine:latest");
    prepare_data.container_config.cmd =
      Some(vec!["sh".to_owned(), "-c".to_owned(), "true".to_owned()]);

    let mut api = container("api", "alpine:latest");
    api.container_config.cmd = Some(vec![
      "sh".to_owned(),
      "-c".to_owned(),
      "sleep 3; touch /tmp/ready; while true; do sleep 3600; done".to_owned(),
    ]);
    api.container_config.healthcheck = Some(HealthConfig {
      test: Some(vec![
        "CMD-SHELL".to_owned(),
        "test -f /tmp/ready".to_owned(),
      ]),
      interval: Some(500_000_000),
      timeout: Some(1_000_000_000),
      retries: Some(3),
      start_period: Some(10_000_000_000),
      ..Default::default()
    });

    let mut worker = container("worker", "alpine:latest");
    worker.container_config.cmd = Some(vec![
      "sh".to_owned(),
      "-c".to_owned(),
      "while true; do sleep 3600; done".to_owned(),
    ]);

    let mut metrics = container("metrics", "alpine:latest");
    metrics.essential = false;
    metrics.container_config.cmd = Some(vec![
      "sh".to_owned(),
      "-c".to_owned(),
      "while true; do sleep 3600; done".to_owned(),
    ]);
    metrics.container_config.healthcheck = Some(HealthConfig {
      test: Some(vec!["CMD-SHELL".to_owned(), "false".to_owned()]),
      interval: Some(500_000_000),
      timeout: Some(1_000_000_000),
      retries: Some(1),
      ..Default::default()
    });

    let replacement = CargoSpec {
      name: NAME.to_owned(),
      init_containers: vec![prepare_schema, prepare_data],
      containers: vec![api, worker, metrics],
      ..Default::default()
    };
    let start_events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        KEY,
        EventKind::Normal,
        NativeEventAction::Start,
      )]))
      .await
      .unwrap();
    let completion_events = system
      .state
      .subscribe_raw(Some(vec![EventCondition {
        actor_key: Some(KEY.to_owned()),
        actor_kind: Some(EventActorKind::Cargo),
        related_key: None,
        related_kind: None,
        kind: vec![EventKind::Normal, EventKind::Error],
        action: vec![NativeEventAction::Update],
      }]))
      .await
      .unwrap();
    let response = system
      .client
      .send_put(
        &format!("{ENDPOINT}/{KEY}"),
        Some(&replacement),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::OK,
      "put final multi-container Cargo declaration"
    );
    let updating = response.json::<Cargo>().await.unwrap();
    assert_eq!(updating.status.actual, ObjPsStatusKind::Updating);
    assert_eq!(updating.spec.init_containers, replacement.init_containers);
    assert_eq!(updating.spec.containers, replacement.containers);

    let events = collect_events(start_events).await;
    assert!(events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Start.to_string()
    }));

    // Assert the complete readiness boundary at the promotion event, before
    // waiting for the four-second retained-generation cleanup.
    let response = system
      .client
      .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
      .await;
    let at_promotion = response.json::<CargoInspect>().await.unwrap();
    assert_eq!(at_promotion.status.actual, ObjPsStatusKind::Start);
    assert_eq!(at_promotion.status.health, ObjPsHealthStatusKind::Healthy);
    assert!(at_promotion.instances.iter().any(|process| {
      process.name.ends_with(".tmp.c")
        && process.data.state.as_ref().and_then(|state| state.running)
          == Some(true)
    }));
    let sandbox = active_cargo_process(&at_promotion, "sandbox", "_sandbox");
    assert_eq!(
      sandbox.data.state.as_ref().and_then(|state| state.running),
      Some(true)
    );
    let api = active_cargo_process(&at_promotion, "app", "api");
    assert_eq!(
      api.data.state.as_ref().and_then(|state| state.running),
      Some(true)
    );
    assert_eq!(
      api
        .data
        .state
        .as_ref()
        .and_then(|state| state.health.as_ref())
        .and_then(|health| health.status.as_ref())
        .map(ToString::to_string)
        .as_deref(),
      Some("healthy")
    );
    let worker = active_cargo_process(&at_promotion, "app", "worker");
    assert_eq!(
      worker.data.state.as_ref().and_then(|state| state.running),
      Some(true)
    );
    let metrics = active_cargo_process(&at_promotion, "app", "metrics");
    assert_eq!(
      metrics
        .data
        .state
        .as_ref()
        .and_then(|state| state.health.as_ref())
        .and_then(|health| health.status.as_ref())
        .map(ToString::to_string)
        .as_deref(),
      Some("unhealthy")
    );
    let events = collect_events(completion_events).await;
    assert!(events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Update.to_string()
    }));
    assert!(!events.iter().any(|event| {
      event.kind == EventKind::Error
        && event.action == NativeEventAction::Update.to_string()
    }));
    system
      .state
      .inner
      .task_manager
      .wait_task(&format!("{}@{KEY}", EventActorKind::Cargo))
      .await;

    let response = system
      .client
      .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::OK,
      "inspect promoted multi-container Cargo"
    );
    let cargo = response.json::<CargoInspect>().await.unwrap();
    assert_eq!(cargo.status.actual, ObjPsStatusKind::Start);
    assert_eq!(cargo.status.health, ObjPsHealthStatusKind::Healthy);
    assert_eq!(cargo.spec.init_containers, replacement.init_containers);
    assert_eq!(cargo.spec.containers, replacement.containers);
    assert_eq!(cargo.instance_total, 6);
    assert!(cargo.instances.iter().all(|process| {
      !process.name.ends_with(".tmp.c")
        && !process.name.ends_with(".candidate.c")
    }));
    let init_states = cargo
      .instances
      .iter()
      .filter_map(|process| {
        let labels = process.data.config.as_ref()?.labels.as_ref()?;
        (labels.get(LABEL_ROLE).map(String::as_str) == Some("init")).then(
          || {
            (
              labels.get(LABEL_CONTAINER).unwrap().as_str(),
              process.data.state.as_ref().unwrap(),
            )
          },
        )
      })
      .collect::<std::collections::HashMap<_, _>>();
    for name in ["prepare-schema", "prepare-data"] {
      let state = init_states
        .get(name)
        .unwrap_or_else(|| panic!("missing completed init container {name}"));
      assert_eq!(
        state.status.as_ref().map(ToString::to_string).as_deref(),
        Some("exited")
      );
      assert_eq!(state.exit_code, Some(0));
    }
    let first_finished = init_states["prepare-schema"]
      .finished_at
      .as_deref()
      .expect("first init must have a finish time");
    let second_started = init_states["prepare-data"]
      .started_at
      .as_deref()
      .expect("second init must have a start time");
    assert!(
      first_finished <= second_started,
      "init containers must execute in declaration order"
    );

    let response = system
      .client
      .send_delete(
        &format!("{ENDPOINT}/{KEY}"),
        Some(CargoDeleteQuery { force: Some(true) }),
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::ACCEPTED,
      "delete multi-container update fixture"
    );
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn failed_essential_candidate_does_not_emit_successful_promotion() {
    const NAME: &str = "failed-essential-update-readiness";
    const KEY: &str = "global.failed-essential-update-readiness";

    let system = gen_default_test_system().await;
    let response = system
      .client
      .send_post(
        ENDPOINT,
        Some(cargo_spec(
          NAME,
          "ghcr.io/next-hat/nanocl-get-started:latest",
        )),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::CREATED,
      "create failed essential update fixture"
    );
    start_cargo(&system, KEY).await;

    let mut failing = container("api", "alpine:latest");
    failing.container_config.cmd = Some(vec![
      "sh".to_owned(),
      "-c".to_owned(),
      "while true; do sleep 3600; done".to_owned(),
    ]);
    failing.container_config.healthcheck = Some(HealthConfig {
      test: Some(vec!["CMD-SHELL".to_owned(), "false".to_owned()]),
      interval: Some(500_000_000),
      timeout: Some(1_000_000_000),
      retries: Some(1),
      ..Default::default()
    });
    let replacement = CargoSpec {
      name: NAME.to_owned(),
      containers: vec![failing],
      ..Default::default()
    };
    let failure_events = system
      .state
      .subscribe_raw(Some(vec![cargo_event_condition(
        KEY,
        EventKind::Error,
        NativeEventAction::Update,
      )]))
      .await
      .unwrap();
    let response = system
      .client
      .send_put(
        &format!("{ENDPOINT}/{KEY}"),
        Some(&replacement),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::OK,
      "put failing essential Cargo declaration"
    );
    assert_eq!(
      response.json::<Cargo>().await.unwrap().status.actual,
      ObjPsStatusKind::Updating
    );

    let events = collect_events(failure_events).await;
    assert!(events.iter().any(|event| {
      event.kind == EventKind::Error
        && event.action == NativeEventAction::Update.to_string()
    }));
    assert!(!events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Update.to_string()
    }));
    assert!(!events.iter().any(|event| {
      event.kind == EventKind::Normal
        && event.action == NativeEventAction::Start.to_string()
        && event.actor.as_ref().is_some_and(|actor| {
          actor.kind == EventActorKind::Cargo
            && actor.key.as_deref() == Some(KEY)
        })
    }));

    let response = system
      .client
      .send_get(&format!("{ENDPOINT}/{KEY}/inspect"), None::<String>)
      .await;
    let cargo = response.json::<CargoInspect>().await.unwrap();
    assert_eq!(cargo.status.actual, ObjPsStatusKind::Fail);

    let response = system
      .client
      .send_delete(
        &format!("{ENDPOINT}/{KEY}"),
        Some(CargoDeleteQuery { force: Some(true) }),
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::ACCEPTED,
      "delete failed essential update fixture"
    );
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn test_ssl() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_post(
        "/secrets",
        Some(SecretPartial {
          name: "test-secret-ssl".to_owned(),
          kind: "nanocl.io/tls".to_owned(),
          immutable: false,
          // Some random metadata
          metadata: Some(serde_json::json!({
            "nginx": "ssl",
          })),
          data: serde_json::to_value(&ProxySslConfig {
            certificate: "test random data".to_owned(),
            certificate_key: "test random data".to_owned(),
            certificate_client: Some("test random data".to_owned()),
            verify_client: None,
            dhparam: None,
          })
          .unwrap(),
        }),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create secret");
    // create a cargo using the secret
    let mut spec = cargo_spec(
      "test-cargo-ssl",
      "ghcr.io/next-hat/nanocl-get-started:latest",
    );
    spec.secrets = vec!["test-secret-ssl".to_owned()];
    let res = client
      .send_post(ENDPOINT, Some(&spec), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create cargo");
    let cargo = TestClient::res_json::<Cargo>(res).await;
    assert_eq!(cargo.spec.name, "test-cargo-ssl", "Invalid cargo name");
    assert_eq!(cargo.spec.containers, spec.containers);
    // start the cargo
    let res = client
      .send_post(
        "/processes/cargo/global.test-cargo-ssl/start",
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(res.status(), http::StatusCode::ACCEPTED, "start cargo");
    // Watch event for the cargo to be sure it started
    let res = client
      .send_post(
        "/events/watch",
        Some(vec![EventCondition {
          actor_key: Some("global.test-cargo-ssl".to_owned()),
          actor_kind: Some(EventActorKind::Cargo),
          related_key: None,
          related_kind: None,
          kind: vec![EventKind::Normal],
          action: vec![NativeEventAction::Start],
        }]),
        None::<String>,
      )
      .await;
    assert_eq!(res.status(), http::StatusCode::OK, "watch cargo");
    let mut stream = res.into_stream();
    while let Some(_chunk) = stream.next().await {}
    // Delete the cargo
    let res = client
      .send_delete(
        &format!("{ENDPOINT}/global.test-cargo-ssl"),
        Some(CargoDeleteQuery { force: Some(true) }),
      )
      .await;
    assert_eq!(res.status(), http::StatusCode::ACCEPTED, "delete cargo");
    // delete the secret
    let res = client
      .send_delete("/secrets/test-secret-ssl", None::<String>)
      .await;
    assert_eq!(res.status(), http::StatusCode::ACCEPTED, "delete secret");
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }
}
