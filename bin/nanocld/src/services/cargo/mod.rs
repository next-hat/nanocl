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

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_cargo);
  config.service(delete_cargo);
  config.service(patch_cargo);
  config.service(put_cargo);
  config.service(list_cargo);
  config.service(inspect_cargo);
  config.service(list_cargo_history);
  config.service(revert_cargo);
  config.service(count_cargo);
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
    proxy::ProxySslConfig,
    secret::SecretPartial,
    system::{
      Event, EventActorKind, EventCondition, EventKind, NativeEventAction,
      ObjPsHealthStatusKind, ObjPsStatusKind,
    },
  };

  use crate::utils::{
    container::cargo_compiler::{LABEL_CONTAINER, LABEL_ROLE},
    tests::*,
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
        if process.name.starts_with("tmp-")
          || process.name.starts_with("candidate-")
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

  #[ntex::test]
  async fn collection_namespace_and_key_semantics() {
    const NAME: &str = "same-name-namespace-test";
    const NAMESPACE: &str = "cargo-collection-test";
    const GLOBAL_KEY: &str = "global.same-name-namespace-test";
    const OTHER_KEY: &str = "cargo-collection-test.same-name-namespace-test";

    let system = gen_default_test_system().await;
    let client = system.client;
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

    for key in [GLOBAL_KEY, OTHER_KEY] {
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
    }
  }

  /// Test to create start patch stop and delete a cargo with valid data
  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
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
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_key}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo start"
    );
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
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_key}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo restart"
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
    for test_cargo in test_cargoes.iter() {
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
    let res = client
      .send_post(
        "/processes/cargo/global.init-test-cargo/start",
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(res.status(), http::StatusCode::ACCEPTED, "init cargo start");
    let res = client
      .send_post(
        "/events/watch",
        Some(vec![EventCondition {
          actor_key: Some("global.init-test-cargo".to_owned()),
          actor_kind: Some(EventActorKind::Cargo),
          related_key: None,
          related_kind: None,
          kind: vec![EventKind::Normal],
          action: vec![NativeEventAction::Start],
        }]),
        None::<String>,
      )
      .await;
    assert_eq!(res.status(), http::StatusCode::OK, "init cargo watch");
    let mut stream = res.into_stream();
    while let Some(_chunk) = stream.next().await {}
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
      process.name.starts_with("tmp-")
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
      !process.name.starts_with("tmp-")
        && !process.name.starts_with("candidate-")
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
