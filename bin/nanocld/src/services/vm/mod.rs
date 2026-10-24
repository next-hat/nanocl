use ntex::web;

pub(crate) mod console;
pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod lifecycle;
pub mod list;
pub mod list_history;
pub mod patch;
pub mod resize;

pub use console::*;
pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use lifecycle::*;
pub use list::*;
pub use list_history::*;
pub use patch::*;
pub use resize::*;

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  config.service(
    web::resource("/vms/{key}/resize")
      .route(web::post().to_with_state(resize_vm)),
  );
  config.service(
    web::resource("/vms/{key}/resources")
      .route(web::get().to_with_state(vm_resources)),
  );
  config.service(
    web::resource("/vms/{key}/stop").route(web::post().to_with_state(stop_vm)),
  );
  config.service(
    web::resource("/vms/{key}/restart")
      .route(web::post().to_with_state(restart_vm)),
  );
  config.service(
    web::resource("/vms/{key}/status")
      .route(web::get().to_with_state(vm_status)),
  );
  config.service(
    web::resource("/vms/{key}/console")
      .route(web::get().to_with_state(console_vm)),
  );
  config.service(
    web::resource("/vms")
      .name("list_vm")
      .guard(web::guard::Get())
      .to_with_state(list_vm),
  );
  config.service(
    web::resource("/vms")
      .name("create_vm")
      .guard(web::guard::Post())
      .to_with_state(create_vm),
  );
  config.service(
    web::resource("/vms/{key}")
      .name("delete_vm")
      .guard(web::guard::Delete())
      .to_with_state(delete_vm),
  );
  config.service(
    web::resource("/vms/{key}/inspect")
      .name("inspect_vm")
      .guard(web::guard::Get())
      .to_with_state(inspect_vm),
  );
  config.service(
    web::resource("/vms/count")
      .name("count_vm")
      .guard(web::guard::Get())
      .to_with_state(count_vm),
  );
  config.service(
    web::resource("/vms/{key}/histories")
      .name("list_vm_history")
      .guard(web::guard::Get())
      .to_with_state(list_vm_history),
  );
  config.service(
    web::resource("/vms/{key}")
      .name("patch_vm")
      .guard(web::guard::Patch())
      .to_with_state(patch_vm),
  );
}

#[cfg(test)]
mod tests {
  use futures::StreamExt;
  use nanocl_stubs::{
    generic::{
      GenericClause, GenericCount, GenericFilter, GenericListQueryNsp,
      GenericNspQuery,
    },
    namespace::NamespacePartial,
    system::{
      Event, EventActorKind, EventCondition, EventKind, NativeEventAction,
    },
    vm::{VmInspect, VmSummary},
    vm_spec::{VmSpecPartial, VmSpecUpdate},
  };
  use ntex::http;

  use crate::utils::tests::*;

  async fn vm_task_registration(
    state: &crate::models::SystemState,
    key: &str,
    action: NativeEventAction,
  ) -> crate::models::RawEventReceiver {
    state
      .subscribe_raw(Some(vec![EventCondition {
        actor_key: Some(key.to_owned()),
        actor_kind: Some(EventActorKind::Vm),
        kind: vec![EventKind::Normal],
        action: vec![action],
        ..Default::default()
      }]))
      .await
      .unwrap()
  }

  fn is_vm_task_registration(
    bytes: &[u8],
    key: &str,
    action: NativeEventAction,
  ) -> bool {
    if bytes.is_empty() {
      return false;
    }
    let event: Event = serde_json::from_slice(bytes).unwrap();
    EventCondition {
      actor_key: Some(key.to_owned()),
      actor_kind: Some(EventActorKind::Vm),
      kind: vec![EventKind::Normal],
      action: vec![action],
      ..Default::default()
    } == event
  }

  async fn wait_vm_task(
    mut registration: crate::models::RawEventReceiver,
    state: &crate::models::SystemState,
    key: &str,
    action: NativeEventAction,
  ) {
    ntex::time::timeout(std::time::Duration::from_secs(90), async {
      // Raw subscriptions deliver every event; their conditions only stop
      // the stream. Match this operation before observing its registered task.
      loop {
        let event = registration
          .next()
          .await
          .expect("VM event stream ended")
          .unwrap();
        if is_vm_task_registration(&event, key, action.clone()) {
          break;
        }
      }
      state
        .inner
        .task_manager
        .wait_task(&format!("Vm@{key}"))
        .await;
    })
    .await
    .expect("VM task did not settle");
  }

  #[test]
  fn vm_task_registration_ignores_unrelated_raw_events() {
    const KEY: &str = "global.example";
    let event = |key: &str, actor_kind: &str, action: &str, kind: &str| {
      serde_json::to_vec(&serde_json::json!({
        "Key": uuid::Uuid::nil(),
        "CreatedAt": "2026-01-01T00:00:00",
        "ExpiresAt": "2026-01-01T00:01:00",
        "ReportingNode": "test",
        "ReportingController": "nanocl.io/core",
        "Kind": kind,
        "Action": action,
        "Reason": "test",
        "Actor": { "Key": key, "Kind": actor_kind },
      }))
      .unwrap()
    };
    let matches = |bytes: &[u8]| {
      is_vm_task_registration(bytes, KEY, NativeEventAction::Destroying)
    };
    assert!(!matches(b""));
    assert!(!matches(&event(KEY, "Vm", "update", "normal")));
    assert!(!matches(&event("other.vm", "Vm", "destroying", "normal")));
    assert!(!matches(&event(KEY, "Cargo", "destroying", "normal")));
    assert!(!matches(&event(KEY, "Vm", "destroying", "error")));
    assert!(matches(&event(KEY, "Vm", "destroying", "normal")));
  }

  #[ntex::test]
  async fn collection_namespace_and_key_semantics() {
    const NAME: &str = "same-name-vm-test";
    const NAMESPACE: &str = "vm-collection-test";
    const GLOBAL_KEY: &str = "global.same-name-vm-test";
    const OTHER_KEY: &str = "vm-collection-test.same-name-vm-test";

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

    let spec = VmSpecPartial {
      name: NAME.to_owned(),
      // A missing bind source can become a directory and briefly start a
      // runtime without QMP. A character-device parent prevents that race.
      image: "/dev/null/namespace-test-vm.img".to_owned(),
      ..Default::default()
    };
    for namespace in [None, Some(NAMESPACE)] {
      let response = client
        .send_post("/vms", Some(&spec), Some(GenericNspQuery::new(namespace)))
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
      let response = client.send_get("/vms", Some(query(namespace))).await;
      let vms = response.json::<Vec<VmSummary>>().await.unwrap();
      let keys = vms
        .iter()
        .map(|vm| vm.spec.vm_key.as_str())
        .collect::<Vec<_>>();
      assert!(keys.contains(&GLOBAL_KEY));
      assert!(keys.contains(&OTHER_KEY));

      let response =
        client.send_get("/vms/count", Some(query(namespace))).await;
      let count = response.json::<GenericCount>().await.unwrap();
      assert_eq!(count.count as usize, vms.len());
    }

    for (namespace, expected_key) in
      [("global", GLOBAL_KEY), (NAMESPACE, OTHER_KEY)]
    {
      let response =
        client.send_get("/vms", Some(query(Some(namespace)))).await;
      let vms = response.json::<Vec<VmSummary>>().await.unwrap();
      assert_eq!(vms.len(), 1);
      assert_eq!(vms[0].spec.vm_key, expected_key);

      let response = client
        .send_get("/vms/count", Some(query(Some(namespace))))
        .await;
      let count = response.json::<GenericCount>().await.unwrap();
      assert_eq!(count.count, 1);
    }

    let response = client
      .send_get(&format!("/vms/{OTHER_KEY}/inspect"), None::<String>)
      .await;
    let vm = response.json::<VmInspect>().await.unwrap();
    assert_eq!(vm.namespace_name, NAMESPACE);

    let response = client
      .send_get("/vms/not-a-key/inspect", None::<String>)
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::BAD_REQUEST,
      "invalid VM key"
    );

    let response = client
      .send_patch(
        &format!("/vms/{OTHER_KEY}"),
        Some(VmSpecUpdate {
          name: Some("renamed".to_owned()),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::BAD_REQUEST,
      "VM name is immutable"
    );

    let update = vm_task_registration(
      &system.state,
      OTHER_KEY,
      NativeEventAction::Updating,
    )
    .await;
    let response = client
      .send_patch(
        &format!("/vms/{OTHER_KEY}"),
        Some(VmSpecUpdate {
          hostname: Some("updated".to_owned()),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::OK,
      "patch VM by canonical key"
    );
    wait_vm_task(
      update,
      &system.state,
      OTHER_KEY,
      NativeEventAction::Updating,
    )
    .await;

    for key in [GLOBAL_KEY, OTHER_KEY] {
      let deletion =
        vm_task_registration(&system.state, key, NativeEventAction::Destroying)
          .await;
      let response = client
        .send_delete(&format!("/vms/{key}"), None::<String>)
        .await;
      test_status_code!(
        response.status(),
        http::StatusCode::OK,
        "delete VM by canonical key"
      );
      wait_vm_task(deletion, &system.state, key, NativeEventAction::Destroying)
        .await;
      let response = client
        .send_get(&format!("/vms/{key}/inspect"), None::<String>)
        .await;
      test_status_code!(
        response.status(),
        http::StatusCode::NOT_FOUND,
        "VM deletion completed"
      );
    }
  }

  // Exercises daemon/database replacement handling with an owned sleeping
  // container. This is a runtime model, not guest, QMP, or browser validation.
  #[ntex::test]
  async fn runtime_model_replacement_dns_and_preflight_recovery() {
    use crate::{
      models::{ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb, VmDb},
      repositories::generic::*,
    };
    use bollard_next::container::{
      Config, CreateContainerOptions, RemoveContainerOptions,
      StartContainerOptions,
    };
    use futures::FutureExt;
    use nanocl_stubs::{
      generic::ImagePullPolicy,
      process::ProcessKind,
      system::ObjPsStatusKind,
      vm::Vm,
      vm_spec::{VmHostConfig, VmHostConfigUpdate},
    };
    use std::collections::HashMap;

    const IMAGE: &str = "alpine:3.23";
    let system = gen_default_test_system().await;
    let state = &system.state;
    let docker = &state.inner.docker_api;
    let name = format!("runtime-model-{}", uuid::Uuid::new_v4().simple());
    let key = format!("global.{name}");
    let response = system
      .client
      .send_post(
        "/vms",
        Some(VmSpecPartial {
          name,
          image: "/tmp/runtime-model-unused.img".into(),
          cloud_init: Some(false),
          host_config: Some(VmHostConfig {
            runtime_network: Some("bridge".into()),
            ..Default::default()
          }),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::OK,
      "create runtime-model VM metadata"
    );

    let mut runtime_id = None;
    let result = std::panic::AssertUnwindSafe(async {
      let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      crate::utils::container::image::download(
        IMAGE,
        None,
        ImagePullPolicy::IfNotPresent,
        &vm,
        state,
      )
      .await
      .expect("download the runtime-model image when absent");
      let runtime = docker
        .create_container(
          Some(CreateContainerOptions {
            name: format!("{key}.v"),
            ..Default::default()
          }),
          Config {
            image: Some(IMAGE.to_owned()),
            cmd: Some(vec!["sleep".to_owned(), "600".to_owned()]),
            labels: Some(HashMap::from([
              ("io.nanocl.v".to_owned(), key.clone()),
              ("io.nanocl.n".to_owned(), "global".to_owned()),
              ("io.nanocl.vm-spec".to_owned(), vm.spec.key.to_string()),
            ])),
            ..Default::default()
          },
        )
        .await
        .unwrap();
      runtime_id = Some(runtime.id.clone());
      docker
        .start_container(&runtime.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
      let inspect = docker.inspect_container(&runtime.id, None).await.unwrap();
      assert_eq!(
        inspect.state.as_ref().and_then(|state| state.running),
        Some(true)
      );
      let address = inspect
        .network_settings
        .as_ref()
        .unwrap()
        .networks
        .as_ref()
        .unwrap()
        .get("bridge")
        .unwrap()
        .ip_address
        .clone()
        .filter(|address| !address.is_empty())
        .unwrap();
      let now = chrono::Utc::now().naive_utc();
      ProcessDb::create_from(
        ProcessDb {
          key: runtime.id.clone(),
          name: format!("{key}.v"),
          kind: ProcessKind::Vm.to_string(),
          kind_key: key.clone(),
          node_name: state.inner.config.hostname.clone(),
          data: serde_json::to_value(&inspect).unwrap(),
          created_at: now,
          updated_at: now,
        },
        &state.inner.pool,
      )
      .await
      .unwrap();
      ObjPsStatusDb::update_pk(
        &key,
        ObjPsStatusUpdate {
          wanted: Some(ObjPsStatusKind::Start.to_string()),
          actual: Some(ObjPsStatusKind::Start.to_string()),
          ..Default::default()
        },
        &state.inner.pool,
      )
      .await
      .unwrap();
      let before = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();

      let rejected = system
        .client
        .send_patch(
          &format!("/vms/{key}"),
          Some(VmSpecUpdate {
            host_config: Some(VmHostConfigUpdate {
              dns: Some(Some(vec![address])),
              ..Default::default()
            }),
            ..Default::default()
          }),
          None::<String>,
        )
        .await;
      test_status_code!(
        rejected.status(),
        http::StatusCode::BAD_REQUEST,
        "self DNS rejected before acceptance"
      );
      let after_rejection = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(
        after_rejection.spec.key, before.spec.key,
        "rejection must not replace desired revision"
      );
      assert_eq!(
        serde_json::to_value(&after_rejection.status).unwrap(),
        serde_json::to_value(&before.status).unwrap(),
        "rejection must not record a transition"
      );
      assert!(after_rejection.spec.host_config.dns.is_none());

      let registration =
        vm_task_registration(state, &key, NativeEventAction::Updating).await;
      let accepted = system
        .client
        .send_patch(
          &format!("/vms/{key}"),
          Some(VmSpecUpdate {
            hostname: Some("failed-replacement".into()),
            host_config: Some(VmHostConfigUpdate {
              runtime: Some(Some(IMAGE.to_owned())),
              ..Default::default()
            }),
            ..Default::default()
          }),
          None::<String>,
        )
        .await;
      test_status_code!(
        accepted.status(),
        http::StatusCode::OK,
        "accept replacement before runtime capability preflight"
      );
      let accepted = accepted.json::<Vm>().await.unwrap();
      assert_ne!(accepted.spec.key, before.spec.key);
      wait_vm_task(registration, state, &key, NativeEventAction::Updating)
        .await;
      let after_failure = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(
        after_failure.spec.key, accepted.spec.key,
        "failed preflight must retain the accepted desired revision"
      );
      assert_eq!(
        after_failure.spec.hostname.as_deref(),
        Some("failed-replacement")
      );
      assert_eq!(
        after_failure.status.actual,
        ObjPsStatusKind::Start,
        "the original runtime is still running"
      );
      let current = docker
        .inspect_container(&format!("{key}.v"), None)
        .await
        .unwrap();
      assert_eq!(current.id.as_deref(), Some(runtime.id.as_str()));
      assert_eq!(
        current.state.as_ref().and_then(|state| state.running),
        Some(true)
      );
    })
    .catch_unwind()
    .await;

    // Cleanup is tied to this test's immutable ID, even when an assertion fails.
    if let Some(id) = runtime_id {
      docker
        .remove_container(
          &id,
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
        )
        .await
        .unwrap();
      ProcessDb::del_by_pk(&id, &state.inner.pool).await.unwrap();
    }
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let name = "api-test-vm";
    let key = "global.api-test-vm";
    let image = "/tmp/ubuntu-22-test.img";
    let res = client
      .post("/vms")
      .send_json(&VmSpecPartial {
        name: name.to_owned(),
        image: image.to_owned(),
        ..Default::default()
      })
      .await
      .unwrap();
    let status = res.status();
    if status != http::StatusCode::OK {
      let body = res.json::<serde_json::Value>().await.unwrap();
      panic!("create vm failed: {} {}", status, body);
    }
    test_status_code!(res.status(), http::StatusCode::OK, "create vm");
    let res = client
      .get(&format!("/vms/{key}/inspect"))
      .send()
      .await
      .unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "inspect vm");
    let vm = res.json::<VmInspect>().await.unwrap();
    assert_eq!(vm.spec.name, name);
    let res = client.get("/vms").send().await.unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "list vm");
    let vms = res.json::<Vec<VmSummary>>().await.unwrap();
    assert!(vms.iter().any(|i| i.spec.name == name));
    let res = client.delete(&format!("/vms/{key}")).send().await.unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "delete vm");
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }
}
