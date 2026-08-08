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
    system::{EventActorKind, EventCondition, EventKind, NativeEventAction},
  };

  use crate::utils::tests::*;

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
        cargo_spec(&test_cargo, "ghcr.io/next-hat/nanocl-get-started:latest");
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
