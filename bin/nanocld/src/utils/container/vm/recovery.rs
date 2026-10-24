use std::{sync::Arc, time::Duration};

use crate::{
  models::{OperationDb, ProcessDb, SystemState, VmDb},
  repositories::generic::*,
  utils,
};
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  process::ProcessKind,
  system::{NativeEventAction, ObjPsStatusKind},
  vm::Vm,
};
use serde_json::{Value, json};

fn conflict(message: &str) -> IoError {
  IoError::invalid_data("VM operation recovery", message)
}

pub(crate) async fn observe(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<(ObjPsStatusKind, Value)> {
  let processes =
    ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
      .await?;
  if processes.iter().any(|process| {
    process.kind != ProcessKind::Vm
      || process.node_name != state.inner.config.hostname
      || process.key.is_empty()
  }) {
    return Err(conflict(
      "Runtime process ownership is not local and unambiguous",
    ));
  }
  ensure_auxiliary_stopped(vm, state).await?;
  let name = format!("{}.v", vm.spec.vm_key);
  let processes = processes
    .iter()
    .filter(|process| process.name == name)
    .collect::<Vec<_>>();
  let runtime = super::lifecycle::inspect_runtime(vm, state).await?;
  let Some(runtime) = runtime else {
    // A mutable-name miss must not hide an immutable runtime that was renamed.
    for process in processes {
      match state
        .inner
        .docker_api
        .inspect_container(&process.key, None)
        .await
      {
        Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 404,
          ..
        }) => {}
        Ok(_) => {
          return Err(conflict(
            "An owned runtime still exists under a different name",
          ));
        }
        Err(error) => {
          return Err(conflict(&format!(
            "Runtime inspection unavailable: {error}"
          )));
        }
      }
    }
    return Ok((
      ObjPsStatusKind::Stop,
      json!({"Runtime": "absent", "ContainerId": null, "Specification": null, "StartedAt": null}),
    ));
  };
  let generation = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref())
    .and_then(|labels| labels.get("io.nanocl.vm-spec"))
    .and_then(|value| uuid::Uuid::parse_str(value).ok())
    .ok_or_else(|| {
      conflict("Runtime specification generation is missing or invalid")
    })?;
  super::validate_generation(vm, generation, state).await?;
  let running = runtime
    .state
    .as_ref()
    .and_then(|state| state.running)
    .ok_or_else(|| conflict("Runtime running state is unknown"))?;
  let started_at = runtime
    .state
    .as_ref()
    .and_then(|state| state.started_at.as_deref())
    .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
    .ok_or_else(|| conflict("Runtime current-run identity is unknown"))?;
  let id = super::owned_runtime_id(vm, &runtime)?;
  let confirmed = state
    .inner
    .docker_api
    .inspect_container(&id, None)
    .await
    .map_err(|error| {
      conflict(&format!("Runtime confirmation unavailable: {error}"))
    })?;
  if super::owned_runtime_id(vm, &confirmed)? != id
    || confirmed.state.as_ref().and_then(|state| state.running) != Some(running)
    || confirmed
      .state
      .as_ref()
      .and_then(|state| state.started_at.as_deref())
      != Some(started_at)
  {
    return Err(conflict(
      "Runtime identity or current run changed during recovery",
    ));
  }
  Ok((
    if running {
      ObjPsStatusKind::Start
    } else {
      ObjPsStatusKind::Stop
    },
    json!({
      "Runtime": if running {"running"} else {"stopped"},
      "ContainerId": runtime.id,
      "Specification": generation,
      "AcceptedSpecification": vm.spec.key,
      "AcceptedGenerationObserved": generation == vm.spec.key,
      "StartedAt": runtime.state.as_ref().and_then(|state| state.started_at.clone()),
    }),
  ))
}

// A daemon can disappear while an init container is still provisioning a disk.
// Include containers whose Process row was never written, and temporary or
// renamed VM containers. Their continued execution makes a retry unsafe.
pub(super) async fn ensure_auxiliary_stopped(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<()> {
  let containers = state
    .inner
    .docker_api
    .list_containers(Some(bollard_next::container::ListContainersOptions::<
      String,
    > {
      all: true,
      filters: std::collections::HashMap::from([(
        "label".to_owned(),
        vec![format!("io.nanocl.v={}", vm.spec.vm_key)],
      )]),
      ..Default::default()
    }))
    .await
    .map_err(|error| {
      conflict(&format!("VM container inventory unavailable: {error}"))
    })?;
  for container in containers {
    let id = container.id.filter(|id| !id.is_empty()).ok_or_else(|| {
      conflict("VM container inventory has no immutable identity")
    })?;
    let runtime =
      match state.inner.docker_api.inspect_container(&id, None).await {
        Ok(runtime) => runtime,
        Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 404,
          ..
        }) => continue,
        Err(error) => {
          return Err(conflict(&format!(
            "VM auxiliary inspection unavailable: {error}"
          )));
        }
      };
    let labels = runtime
      .config
      .as_ref()
      .and_then(|config| config.labels.as_ref());
    if runtime.id.as_deref() != Some(&id)
      || labels.and_then(|labels| labels.get("io.nanocl.v"))
        != Some(&vm.spec.vm_key)
      || labels.and_then(|labels| labels.get("io.nanocl.n"))
        != Some(&vm.namespace_name)
    {
      return Err(conflict("VM auxiliary container ownership is ambiguous"));
    }
    if runtime.name.as_deref() == Some(&format!("/{}.v", vm.spec.vm_key))
      && labels
        .and_then(|labels| labels.get("io.nanocl.init-v"))
        .map(String::as_str)
        != Some("true")
    {
      continue;
    }
    if runtime.state.as_ref().and_then(|state| state.running) != Some(false) {
      return Err(conflict(&format!(
        "Owned VM auxiliary container {id} ({:?}) is active or unknown; wait for it to exit or explicitly stop it before retrying",
        runtime.name
      )));
    }
  }
  Ok(())
}

/// Settle exactly one abandoned acceptance. This never starts, stops, replaces,
/// or deletes a runtime. Do not timeout the final database write: its blocking
/// worker must finish before the file and lifecycle guards can be released.
pub(crate) async fn recover_one(
  candidate: &OperationDb,
  state: &SystemState,
) -> IoResult<bool> {
  let lifecycle = super::lifecycle_lock(&candidate.resource_key);
  let lifecycle_guard = match tokio::time::timeout(
    Duration::from_millis(50),
    lifecycle.try_lock_owned(),
  )
  .await
  {
    Ok(Ok(guard)) => guard,
    _ => return Ok(false),
  };
  let Some(current) =
    OperationDb::current("Vm", &candidate.resource_key, &state.inner.pool)
      .await?
  else {
    return Ok(false);
  };
  if candidate != &current || candidate.outcome.is_some() {
    return Ok(false);
  }
  let Some(owner_guard) = utils::operation::abandoned_owner(
    &candidate.owner(),
    &state.inner.config.hostname,
    &state.inner.config.state_dir,
  )?
  else {
    return Ok(false);
  };
  let vm =
    VmDb::transform_read_by_pk(&candidate.resource_key, &state.inner.pool)
      .await?;
  let (live, _, _) =
    VmDb::read_by_pk(&candidate.resource_key, &state.inner.pool).await?;
  if live.incarnation_key != candidate.incarnation_key
    || vm.spec.key != candidate.spec_key
  {
    return Ok(false);
  }
  let action = match candidate.action.as_str() {
    "start" => NativeEventAction::Starting,
    "restart" => NativeEventAction::Restart,
    "update" => NativeEventAction::Updating,
    "delete" => NativeEventAction::Destroying,
    _ => return Err(conflict("Unsupported pending phase")),
  };
  let (actual, mut observation) =
    tokio::time::timeout(Duration::from_secs(5), observe(&vm, state))
      .await
      .map_err(|_| {
        conflict("Runtime observation timed out; recovery remains pending")
      })??;
  observation["Failure"] = json!(
    "The accepted operation lost its owner; no automatic replay was attempted"
  );
  observation["Operation"] = json!(candidate.key);
  observation["Phase"] = json!(candidate.action);
  let lease = (Arc::new(lifecycle_guard), owner_guard);
  let finished = VmDb::finish_operation(
    &current,
    actual,
    "interrupted",
    observation,
    lease.clone(),
    &state.inner.pool,
  )
  .await?;
  if finished
    && let Err(error) =
      utils::operation::cleanup_owner(&lease.1, &state.inner.pool).await
  {
    log::warn!("VM owner cleanup deferred after recovery: {error}");
  }
  if finished {
    state.emit_error_native_action(&vm, action,
      Some(format!("VM operation {} ({}) was interrupted; observed runtime reconciled without replay. Inspect the VM and explicitly retry the request", candidate.key, candidate.action)));
  }
  Ok(finished)
}

/// Startup and background passes share the same idempotent recovery path.
/// A cursor prevents unclassifiable early rows from starving later operations.
pub(crate) fn spawn(state: &SystemState) {
  let state = state.clone();
  ntex::rt::spawn(async move {
    let mut after: Option<uuid::Uuid> = None;
    let mut cleanup_cursor = None;
    loop {
      match recover_pending(&state, after).await {
        Ok(cursor) => after = cursor,
        Err(error) => log::warn!("VM recovery scan deferred: {error}"),
      }
      if let Err(error) = utils::operation::maintenance_step(
        &state.inner.config.state_dir,
        &state.inner.config.hostname,
        &mut cleanup_cursor,
        &state.inner.pool,
      )
      .await
      {
        log::warn!("Operation owner maintenance deferred: {error}");
      }
      ntex::time::sleep(Duration::from_secs(10)).await;
    }
  });
}

pub(crate) async fn recover_pending(
  state: &SystemState,
  after: Option<uuid::Uuid>,
) -> IoResult<Option<uuid::Uuid>> {
  let candidates = OperationDb::pending_for_node(
    "Vm",
    &state.inner.config.hostname,
    after,
    32,
    &state.inner.pool,
  )
  .await?;
  let cursor = if candidates.len() == 32 {
    candidates.last().map(|operation| operation.key)
  } else {
    None
  };
  for candidate in candidates {
    if let Err(error) = recover_one(&candidate, state).await {
      log::warn!("VM {} recovery deferred: {error}", candidate.resource_key);
    }
  }
  Ok(cursor)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::models::{
    DistributedMutexDb, DistributedMutexPartial, OperationGuard, Pool, SpecDb,
    SystemStateInner,
  };
  use futures::FutureExt;
  use nanocl_stubs::vm_spec::{VmHostConfig, VmSpecPartial};

  fn test_state(
    state: &SystemState,
    node: &str,
    pool: Pool,
    docker: bollard_next::Docker,
  ) -> SystemState {
    let mut config = state.inner.config.clone();
    config.hostname = node.to_owned();
    SystemState {
      inner: Arc::new(SystemStateInner {
        pool,
        docker_api: docker,
        config,
        task_manager: Default::default(),
        event_emitter: state.inner.event_emitter.clone(),
        event_emitter_raw: Default::default(),
        arbiter: ntex::rt::Arbiter::current(),
      }),
    }
  }

  async fn accept(
    vm: &Vm,
    phase: &str,
    replacement: Option<SpecDb>,
    state: &SystemState,
  ) -> (Vm, OperationDb, OperationGuard) {
    let mutex = DistributedMutexDb::create_from(
      DistributedMutexPartial {
        action: phase.to_owned(),
        resource_kind: "Vm".into(),
        resource_key: vm.spec.vm_key.clone(),
        node_key: state.inner.config.hostname.clone(),
        acquired_by: "daemon".into(),
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
    let (live, _, _) = VmDb::read_by_pk(&vm.spec.vm_key, &state.inner.pool)
      .await
      .unwrap();
    let (owner, guard) = utils::operation::create_owner(
      mutex.key,
      "Vm",
      &vm.spec.vm_key,
      live.incarnation_key,
      &state.inner.config.hostname,
      &state.inner.config.state_dir,
    )
    .unwrap();
    let action = match phase {
      "starting" => "start",
      "restart" => "restart",
      "updating" => "update",
      "destroying" => "delete",
      _ => panic!("invalid phase"),
    };
    let operation = OperationDb::new(
      &owner,
      "Vm",
      &vm.spec.vm_key,
      replacement.as_ref().map_or(vm.spec.key, |spec| spec.key),
      action,
    );
    let accepted = VmDb::accept_operation(
      vm,
      replacement,
      operation.clone(),
      guard.clone(),
      &state.inner.pool,
    )
    .await
    .unwrap();
    (accepted.vm, accepted.operation, guard)
  }

  async fn create_vm(system: &crate::utils::tests::TestSystem) -> Vm {
    let response = system
      .client
      .send_post(
        "/vms",
        Some(VmSpecPartial {
          name: format!("operation-recovery-{}", uuid::Uuid::new_v4().simple()),
          image: "/tmp/operation-recovery-unused.img".into(),
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
    assert!(response.status().is_success());
    response.json::<Vm>().await.unwrap()
  }

  #[ntex::test]
  async fn interrupted_restart_recovers_stopped_runtime_and_allows_normal_retry()
   {
    use crate::{
      models::{NamespaceDb, VmObjCreateIn},
      objects::generic::ObjCreate,
    };
    use ntex::web;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct DockerFixture {
      vm: Vm,
      running: Arc<Mutex<bool>>,
      requests: Arc<Mutex<Vec<String>>>,
    }

    async fn docker_request(request: web::HttpRequest) -> web::HttpResponse {
      let fixture = request.app_state::<DockerFixture>().unwrap();
      let path = request.path();
      fixture
        .requests
        .lock()
        .unwrap()
        .push(format!("{} {path}", request.method()));
      if path.ends_with("/containers/restart-runtime/start")
        && request.method() == ntex::http::Method::POST
      {
        *fixture.running.lock().unwrap() = true;
        return web::HttpResponse::NoContent().build();
      }
      if request.method() == ntex::http::Method::GET {
        if path.ends_with("/containers/json") {
          return web::HttpResponse::Ok()
            .json(&json!([{"Id": "restart-runtime"}]));
        }
        if path.ends_with("/containers/restart-runtime/json")
          || path.ends_with(&format!(
            "/containers/{}.v/json",
            fixture.vm.spec.vm_key
          ))
        {
          let running = *fixture.running.lock().unwrap();
          return web::HttpResponse::Ok().json(&json!({
            "Id": "restart-runtime",
            "Name": format!("/{}.v", fixture.vm.spec.vm_key),
            "Config": {"Labels": {
              "io.nanocl.v": fixture.vm.spec.vm_key,
              "io.nanocl.n": fixture.vm.namespace_name,
              "io.nanocl.vm-spec": fixture.vm.spec.key.to_string()
            }},
            "State": {
              "Running": running,
              "StartedAt": if running {"2026-01-02T00:00:00Z"} else {"2026-01-01T00:00:00Z"}
            }
          }));
        }
      }
      web::HttpResponse::InternalServerError()
        .json(&json!({"message": "Unexpected Docker request in restart recovery fixture"}))
    }

    fn no_routes(_: &mut web::ServiceConfig<SystemState>) {}
    let system =
      crate::utils::tests::gen_test_system(no_routes, crate::vars::VERSION)
        .await;
    let namespace =
      format!("restart-recovery-{}", uuid::Uuid::new_v4().simple());
    NamespaceDb::create_from(
      NamespaceDb::new(&namespace),
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    let vm = VmDb::fn_create_obj(
      &VmObjCreateIn {
        namespace: namespace.clone(),
        version: crate::vars::VERSION.into(),
        spec: VmSpecPartial {
          name: "guest".into(),
          image: "/restart-recovery-test-unused.qcow2".into(),
          cloud_init: Some(false),
          ..Default::default()
        },
      },
      &system.state,
    )
    .await
    .unwrap();
    let key = vm.spec.vm_key.clone();
    let fixture = DockerFixture {
      vm: vm.clone(),
      running: Default::default(),
      requests: Default::default(),
    };
    let server_fixture = fixture.clone();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(web::WebAppConfig::new().set_state(server_fixture.clone()))
        .default_service(web::route().to(docker_request))
    });
    let state = test_state(
      &system.state,
      &namespace,
      system.state.inner.pool.clone(),
      bollard_next::Docker::connect_with_http(
        &format!("http://{}", server.addr()),
        2,
        bollard_next::API_DEFAULT_VERSION,
      )
      .unwrap(),
    );
    let mut lock_keys = Vec::new();
    let result = std::panic::AssertUnwindSafe(async {
      // Model controller loss after restart committed Starting but before its
      // Docker start request. The real runtime is still stopped.
      let (accepted, operation, guard) =
        accept(&vm, "restart", None, &state).await;
      lock_keys.push(operation.key);
      assert_eq!(accepted.status.actual, ObjPsStatusKind::Starting);
      assert!(
        !recover_one(&operation, &state).await.unwrap(),
        "a live owner cannot be recovered"
      );
      // Collection of the admission mutex still cannot override the durable
      // pending operation, even through the ordinary restart entry point.
      DistributedMutexDb::del_by_pk(&operation.key, &state.inner.pool)
        .await
        .unwrap();
      let rejected =
        super::super::restart_with_options(&key, &Default::default(), &state)
          .await
          .unwrap_err();
      assert_eq!(rejected.inner.kind(), std::io::ErrorKind::WouldBlock);
      assert_eq!(
        OperationDb::current("Vm", &key, &state.inner.pool)
          .await
          .unwrap(),
        Some(operation.clone())
      );
      assert_eq!(
        VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap()
          .status,
        accepted.status
      );
      drop(guard);
      assert!(recover_pending(&state, None).await.unwrap().is_none());
      let recovered = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(recovered.status.actual, ObjPsStatusKind::Stop);
      assert_eq!(recovered.status.wanted, ObjPsStatusKind::Stop);
      let interrupted = OperationDb::current("Vm", &key, &state.inner.pool)
        .await
        .unwrap()
        .unwrap();
      assert_eq!(interrupted.key, operation.key);
      assert_eq!(interrupted.outcome.as_deref(), Some("interrupted"));
      assert_eq!(
        interrupted.diagnostic.as_ref().unwrap()["Runtime"],
        "stopped"
      );
      assert!(!*fixture.running.lock().unwrap());
      assert!(
        fixture
          .requests
          .lock()
          .unwrap()
          .iter()
          .all(|request| request.starts_with("GET ")),
        "recovery must only observe the stopped runtime"
      );
      // An explicit ordinary retry may now start exactly the inspected runtime.
      super::super::restart_with_options(&key, &Default::default(), &state)
        .await
        .unwrap();
      state
        .inner
        .task_manager
        .wait_task(&format!("Vm@{key}"))
        .await;
      let completed = OperationDb::current("Vm", &key, &state.inner.pool)
        .await
        .unwrap()
        .unwrap();
      lock_keys.push(completed.key);
      assert_ne!(completed.key, operation.key);
      assert_eq!(completed.action, "restart");
      assert_eq!(completed.outcome.as_deref(), Some("completed"));
      let running = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(running.status.actual, ObjPsStatusKind::Start);
      assert_eq!(running.status.wanted, ObjPsStatusKind::Start);

      // This fixture cannot provide QMP shutdown. A graceful restart must fail
      // without falling back to a Docker power mutation, preserving both the
      // observed runtime and its restored wanted status.
      let error =
        super::super::restart_with_options(&key, &Default::default(), &state)
          .await
          .unwrap_err();
      assert!(error.to_string().contains("required managed QMP interface"));
      state
        .inner
        .task_manager
        .wait_task(&format!("Vm@{key}"))
        .await;
      let failed = OperationDb::current("Vm", &key, &state.inner.pool)
        .await
        .unwrap()
        .unwrap();
      lock_keys.push(failed.key);
      assert_ne!(failed.key, completed.key);
      assert_eq!(failed.action, "restart");
      assert_eq!(failed.outcome.as_deref(), Some("failed"));
      let preserved = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(preserved.status.actual, ObjPsStatusKind::Start);
      assert_eq!(preserved.status.wanted, ObjPsStatusKind::Start);
      assert!(*fixture.running.lock().unwrap());
      let mutations = fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| !request.starts_with("GET "))
        .cloned()
        .collect::<Vec<_>>();
      assert_eq!(mutations.len(), 1);
      assert!(mutations[0].ends_with("/containers/restart-runtime/start"));
    })
    .catch_unwind()
    .await;
    state
      .inner
      .task_manager
      .wait_task(&format!("Vm@{key}"))
      .await;
    if let Some(current) = OperationDb::current("Vm", &key, &state.inner.pool)
      .await
      .unwrap()
      && current.outcome.is_none()
    {
      VmDb::finish_operation(
        &current,
        ObjPsStatusKind::Stop,
        "interrupted",
        Value::Null,
        (),
        &state.inner.pool,
      )
      .await
      .unwrap();
    }
    cleanup_mutex(&key, &state).await;
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
    NamespaceDb::del_by_pk(&namespace, &state.inner.pool)
      .await
      .unwrap();
    cleanup_locks(&lock_keys, &state);
    system.state.wait_event_loop().await;
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }

  // Real database acceptance and Docker inspection, without starting QEMU.
  // Every scan is restricted to this fixture's unique owner node.
  #[ntex::test]
  async fn vm_recovery_committed_pending_retries_dependencies_and_preserves_successor()
   {
    use diesel::r2d2::ConnectionManager;
    let system = crate::utils::tests::gen_default_test_system().await;
    let node = format!("recovery-test-{}", uuid::Uuid::new_v4());
    let state = test_state(
      &system.state,
      &node,
      system.state.inner.pool.clone(),
      system.state.inner.docker_api.clone(),
    );
    let mut vm = create_vm(&system).await;
    let key = vm.spec.vm_key.clone();
    let mut lock_keys = Vec::new();
    let result = std::panic::AssertUnwindSafe(async {
      for phase in ["starting", "updating", "destroying"] {
        let (accepted, operation, guard) =
          accept(&vm, phase, None, &state).await;
        lock_keys.push(operation.key);
        // Expire this exact row and run the daemon GC predicate. Neither the
        // expired timestamp nor physical removal releases the kernel owner.
        use diesel::prelude::*;
        let operation_key = operation.key;
        crate::repositories::operation::run_query_owned(
          &state.inner.pool,
          guard.clone(),
          move |conn| {
            diesel::update(
              crate::schema::distributed_mutexes::table.find(operation_key),
            )
            .set(
              crate::schema::distributed_mutexes::expires_at
                .eq(chrono::Utc::now() - chrono::Duration::seconds(1)),
            )
            .execute(conn)
            .map_err(|error| {
              IoError::other("Recovery test expiry", &error.to_string())
            })?;
            Ok(())
          },
        )
        .await
        .unwrap();
        let filter = nanocl_stubs::generic::GenericFilter::new()
          .r#where(
            "expires_at",
            nanocl_stubs::generic::GenericClause::Le(
              chrono::Utc::now().to_rfc3339(),
            ),
          )
          .r#where(
            "key",
            nanocl_stubs::generic::GenericClause::Eq(operation.key.to_string()),
          );
        DistributedMutexDb::del_by(&filter, &state.inner.pool)
          .await
          .unwrap();
        assert!(
          DistributedMutexDb::get_lock("Vm", &key, &state.inner.pool)
            .await
            .is_err()
        );
        assert!(!recover_one(&operation, &state).await.unwrap());
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          operation
        );
        let unavailable_docker = test_state(
          &state,
          &node,
          state.inner.pool.clone(),
          bollard_next::Docker::connect_with_unix(
            "/dev/null",
            1,
            bollard_next::API_DEFAULT_VERSION,
          )
          .unwrap(),
        );
        let lifecycle = super::super::lifecycle_lock(&key)
          .try_lock_owned()
          .await
          .unwrap();
        let primary =
          IoError::interrupted("VM test operation", "injected primary failure");
        let primary_message = primary.to_string();
        let (result, settled) = super::super::record_task_outcome(
          &key,
          Err(primary),
          &unavailable_docker,
        )
        .await;
        assert!(!settled);
        assert_eq!(result.unwrap_err().to_string(), primary_message);
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          operation
        );
        assert_eq!(
          VmDb::transform_read_by_pk(&key, &state.inner.pool)
            .await
            .unwrap()
            .status
            .actual,
          accepted.status.actual
        );
        drop(lifecycle);
        drop(guard);
        assert!(recover_one(&operation, &unavailable_docker).await.is_err());
        let unavailable_pool = Pool::builder()
          .min_idle(Some(0))
          .max_size(1)
          .connection_timeout(Duration::from_millis(50))
          .build_unchecked(ConnectionManager::<diesel::PgConnection>::new(
            "invalid-recovery-test-connection-string",
          ));
        let unavailable_db = test_state(
          &state,
          &node,
          unavailable_pool,
          state.inner.docker_api.clone(),
        );
        assert!(recover_one(&operation, &unavailable_db).await.is_err());
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          operation
        );
        assert_eq!(
          VmDb::transform_read_by_pk(&key, &state.inner.pool)
            .await
            .unwrap()
            .status
            .actual,
          accepted.status.actual
        );

        // The startup/background pass completes recovery without a new request.
        assert!(recover_pending(&state, None).await.unwrap().is_none());
        vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap();
        assert_eq!(vm.status.actual, ObjPsStatusKind::Stop);
        assert_eq!(vm.status.wanted, ObjPsStatusKind::Stop);
        let settled = OperationDb::current("Vm", &key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap();
        assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
        assert_eq!(settled.diagnostic.as_ref().unwrap()["Runtime"], "absent");
        assert!(!recover_one(&operation, &state).await.unwrap());
        recover_pending(&state, None).await.unwrap();
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          settled
        );

        let (next_vm, successor, next_guard) =
          accept(&vm, "starting", None, &state).await;
        lock_keys.push(successor.key);
        assert!(!recover_one(&operation, &state).await.unwrap());
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          successor
        );
        assert_eq!(
          DistributedMutexDb::get_lock("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .key,
          successor.key
        );
        assert!(
          state
            .inner
            .task_manager
            .get_task(&format!("Vm@{key}"))
            .await
            .is_none()
        );
        drop(next_guard);
        assert!(recover_one(&successor, &state).await.unwrap());
        vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap();
        assert_eq!(vm.spec.key, next_vm.spec.key);
      }
      // Retained boot revisions remain readable after deletion, but cannot
      // authorize runtime control for a newly created VM using the same name.
      let old = VmDb::read_by_pk(&key, &state.inner.pool).await.unwrap().0;
      let history = OperationDb::current("Vm", &key, &state.inner.pool)
        .await
        .unwrap()
        .unwrap();
      VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
      use crate::objects::generic::ObjCreate;
      let recreated = VmDb::fn_create_obj(
        &crate::models::VmObjCreateIn {
          namespace: vm.namespace_name.clone(),
          version: vm.spec.version.clone(),
          spec: VmSpecPartial::from(vm.spec.clone()),
        },
        &state,
      )
      .await
      .unwrap();
      let new = VmDb::read_by_pk(&key, &state.inner.pool).await.unwrap().0;
      assert_ne!(new.incarnation_key, old.incarnation_key);
      let retained = SpecDb::read_by_pk(&old.spec_key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(retained.incarnation_key, Some(old.incarnation_key));
      assert!(
        super::super::validate_generation(&recreated, old.spec_key, &state)
          .await
          .is_err()
      );
      assert!(!recover_one(&history, &state).await.unwrap());
      assert!(
        OperationDb::current("Vm", &key, &state.inner.pool)
          .await
          .unwrap()
          .is_none()
      );
      assert_eq!(
        OperationDb::get(history.key, &state.inner.pool)
          .await
          .unwrap(),
        Some(history)
      );
      let unchanged = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(unchanged.spec.key, recreated.spec.key);
      assert_eq!(unchanged.status, recreated.status);
    })
    .catch_unwind()
    .await;
    cleanup_mutex(&key, &state).await;
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
    cleanup_locks(&lock_keys, &state);
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }

  async fn cleanup_mutex(key: &str, state: &SystemState) {
    if let Ok(mutex) =
      DistributedMutexDb::get_lock("Vm", key, &state.inner.pool).await
    {
      assert_eq!(mutex.node_key, state.inner.config.hostname);
      DistributedMutexDb::del_by_pk(&mutex.key, &state.inner.pool)
        .await
        .unwrap();
    }
  }

  fn cleanup_locks(keys: &[uuid::Uuid], state: &SystemState) {
    for key in keys {
      let path = std::path::Path::new(&state.inner.config.state_dir)
        .join("run/operations")
        .join(format!("{key}.lock"));
      if let Err(error) = std::fs::remove_file(path) {
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
      }
    }
  }

  // A disposable container models runtime identity and interruption. It is not
  // a real guest, QMP, DHCP, browser-console, or live-resize validation.
  #[ntex::test]
  async fn vm_recovery_observes_old_generation_and_partially_stopped_runtime() {
    use crate::models::{NodeDb, ObjPsStatusDb, ObjPsStatusUpdate};
    use bollard_next::container::{
      Config, CreateContainerOptions, RemoveContainerOptions,
      StartContainerOptions, StopContainerOptions,
    };
    use nanocl_stubs::generic::ImagePullPolicy;
    use std::collections::HashMap;

    async fn recover_with_retries(
      operation: &OperationDb,
      state: &SystemState,
    ) -> IoResult<bool> {
      let before =
        VmDb::transform_read_by_pk(&operation.resource_key, &state.inner.pool)
          .await
          .unwrap();
      let specification = serde_json::to_value(&before.spec).unwrap();
      let mut attempts = 0;
      loop {
        attempts += 1;
        let result = recover_one(operation, state).await;
        if matches!(&result, Ok(true)) {
          return result;
        }
        let retry = match &result {
          Ok(false) => {
            // This isolated fixture still has valid, unowned file evidence;
            // an admission deferral must not hide a changed or live owner.
            assert!(
              utils::operation::abandoned_owner(
                &operation.owner(),
                &state.inner.config.hostname,
                &state.inner.config.state_dir,
              )
              .unwrap()
              .is_some()
            );
            true
          }
          Err(error) => {
            error.context.as_deref() == Some("VM operation recovery")
              && error.inner.kind() == std::io::ErrorKind::InvalidData
              && error.inner.to_string()
                == "Runtime observation timed out; recovery remains pending"
          }
          Ok(true) => unreachable!(),
        };
        // Production retries deferred scans. Verify each unsuccessful attempt
        // preserves acceptance exactly before allowing at most two retries.
        assert_eq!(
          OperationDb::current(
            "Vm",
            &operation.resource_key,
            &state.inner.pool
          )
          .await
          .unwrap()
          .as_ref(),
          Some(operation)
        );
        let current = VmDb::transform_read_by_pk(
          &operation.resource_key,
          &state.inner.pool,
        )
        .await
        .unwrap();
        assert_eq!(serde_json::to_value(&current.spec).unwrap(), specification);
        assert_eq!(current.status, before.status);
        if !retry || attempts == 3 {
          return result;
        }
      }
    }

    const IMAGE: &str = "alpine:3.23";
    let system = crate::utils::tests::gen_default_test_system().await;
    let node = format!("runtime-recovery-test-{}", uuid::Uuid::new_v4());
    let state = test_state(
      &system.state,
      &node,
      system.state.inner.pool.clone(),
      system.state.inner.docker_api.clone(),
    );
    let mut vm = create_vm(&system).await;
    let key = vm.spec.vm_key.clone();
    let docker = &state.inner.docker_api;
    let mut runtime_id = None;
    let mut auxiliary_id = None;
    let mut lock_keys = Vec::new();
    let mut test_node_created = false;
    let result = std::panic::AssertUnwindSafe(async {
      // Process rows reference a registered node. Give this fixture its own
      // node so the real daemon scanner cannot race the controlled recovery.
      let mut test_node = NodeDb::read_by_pk(
        &system.state.inner.config.hostname,
        &state.inner.pool,
      )
      .await
      .unwrap();
      test_node.name = node.clone();
      NodeDb::create_from(test_node, &state.inner.pool)
        .await
        .unwrap();
      test_node_created = true;
      crate::utils::container::image::download(
        IMAGE,
        None,
        ImagePullPolicy::IfNotPresent,
        &vm,
        &state,
      )
      .await
      .unwrap();
      let auxiliary = docker
        .create_container(
          Some(CreateContainerOptions {
            name: format!("{key}.test.init.v"),
            ..Default::default()
          }),
          Config {
            image: Some(IMAGE.to_owned()),
            cmd: Some(vec!["sleep".to_owned(), "600".to_owned()]),
            labels: Some(HashMap::from([
              ("io.nanocl.v".to_owned(), key.clone()),
              ("io.nanocl.n".to_owned(), "global".to_owned()),
              ("io.nanocl.init-v".to_owned(), "true".to_owned()),
            ])),
            ..Default::default()
          },
        )
        .await
        .unwrap();
      auxiliary_id = Some(auxiliary.id.clone());
      docker
        .start_container(&auxiliary.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
      let (_, provisioning, guard) =
        accept(&vm, "starting", None, &state).await;
      lock_keys.push(provisioning.key);
      drop(guard);
      let active_auxiliary = recover_with_retries(&provisioning, &state)
        .await
        .unwrap_err();
      assert_eq!(
        active_auxiliary.context.as_deref(),
        Some("VM operation recovery")
      );
      assert_eq!(
        active_auxiliary.inner.kind(),
        std::io::ErrorKind::InvalidData
      );
      assert_eq!(
        active_auxiliary.inner.to_string(),
        format!(
          "Owned VM auxiliary container {} ({:?}) is active or unknown; wait for it to exit or explicitly stop it before retrying",
          auxiliary.id,
          Some(format!("/{key}.test.init.v"))
        ),
        "a live init without a Process row must block retry"
      );
      assert_eq!(
        OperationDb::current("Vm", &key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap(),
        provisioning
      );
      docker
        .stop_container(&auxiliary.id, Some(StopContainerOptions { t: 0 }))
        .await
        .unwrap();
      assert!(recover_with_retries(&provisioning, &state).await.unwrap());
      docker
        .remove_container(&auxiliary.id, None::<RemoveContainerOptions>)
        .await
        .unwrap();
      auxiliary_id = None;
      vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      let created = docker
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
      runtime_id = Some(created.id.clone());
      docker
        .start_container(&created.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
      let inspect = docker.inspect_container(&created.id, None).await.unwrap();
      let now = chrono::Utc::now().naive_utc();
      ProcessDb::create_from(
        ProcessDb {
          key: created.id.clone(),
          name: format!("{key}.v"),
          kind: ProcessKind::Vm.to_string(),
          kind_key: key.clone(),
          node_name: node.clone(),
          data: serde_json::to_value(&inspect).unwrap(),
          created_at: now,
          updated_at: now,
        },
        &state.inner.pool,
      )
      .await
      .unwrap();
      let mut replacement = SpecDb::read_by_pk(&vm.spec.key, &state.inner.pool)
        .await
        .unwrap();
      replacement.key = uuid::Uuid::new_v4();
      replacement.data["Hostname"] = json!("interrupted-replacement");
      let (accepted, operation, guard) =
        accept(&vm, "updating", Some(replacement), &state).await;
      lock_keys.push(operation.key);
      drop(guard);
      assert!(recover_with_retries(&operation, &state).await.unwrap());
      let current = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(current.spec.key, accepted.spec.key);
      assert_ne!(current.spec.key, vm.spec.key);
      assert_eq!(current.status.actual, ObjPsStatusKind::Start);
      assert_eq!(current.status.wanted, ObjPsStatusKind::Start);
      let settled = OperationDb::current("Vm", &key, &state.inner.pool)
        .await
        .unwrap()
        .unwrap();
      assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
      let observation = settled.diagnostic.unwrap();
      assert_eq!(observation["Specification"], json!(vm.spec.key));
      assert_eq!(observation["AcceptedGenerationObserved"], false);
      assert_eq!(observation["ContainerId"], created.id);
      assert_eq!(
        docker
          .inspect_container(&created.id, None)
          .await
          .unwrap()
          .state
          .unwrap()
          .running,
        Some(true)
      );

      let (_, destroying, guard) =
        accept(&current, "destroying", None, &state).await;
      lock_keys.push(destroying.key);
      // Interruption after runtime work has begun: it is now stopped even
      // though the persisted operation still names its pending destroy phase.
      docker
        .stop_container(&created.id, Some(StopContainerOptions { t: 0 }))
        .await
        .unwrap();
      ObjPsStatusDb::update_pk(
        &key,
        ObjPsStatusUpdate {
          actual: Some(ObjPsStatusKind::Stop.to_string()),
          ..Default::default()
        },
        &state.inner.pool,
      )
      .await
      .unwrap();
      drop(guard);
      assert!(recover_with_retries(&destroying, &state).await.unwrap());
      let current = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(current.status.actual, ObjPsStatusKind::Stop);
      assert_eq!(current.status.wanted, ObjPsStatusKind::Stop);
      assert_eq!(
        OperationDb::current("Vm", &key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap()
          .diagnostic
          .unwrap()["Runtime"],
        "stopped"
      );
      assert!(
        docker.inspect_container(&created.id, None).await.is_ok(),
        "recovery must not replay deletion"
      );

      let (_, starting, guard) =
        accept(&current, "starting", None, &state).await;
      lock_keys.push(starting.key);
      docker
        .remove_container(&created.id, None::<RemoveContainerOptions>)
        .await
        .unwrap();
      ProcessDb::del_by_pk(&created.id, &state.inner.pool)
        .await
        .unwrap();
      runtime_id = None;
      drop(guard);
      assert!(recover_with_retries(&starting, &state).await.unwrap());
      assert_eq!(
        VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap()
          .status
          .actual,
        ObjPsStatusKind::Stop
      );
      assert!(!recover_one(&starting, &state).await.unwrap());
    })
    .catch_unwind()
    .await;

    if let Some(id) = auxiliary_id {
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
    }
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
    cleanup_mutex(&key, &state).await;
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
    cleanup_locks(&lock_keys, &state);
    if test_node_created {
      NodeDb::del_by_pk(&node, &state.inner.pool).await.unwrap();
    }
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }

  #[test]
  #[ignore = "subprocess helper invoked by the controller-exit recovery test"]
  fn vm_recovery_owner_process() {
    use std::io::{Read, Write};

    let value = |key| std::env::var(key).unwrap();
    let key = value("NANOCL_RECOVERY_TEST_VM");
    let node = value("NANOCL_RECOVERY_TEST_NODE");
    let state_dir = value("NANOCL_RECOVERY_TEST_STATE");
    let operation = value("NANOCL_RECOVERY_TEST_OPERATION").parse().unwrap();
    let incarnation =
      value("NANOCL_RECOVERY_TEST_INCARNATION").parse().unwrap();
    let (owner, _guard) = match std::env::var("NANOCL_RECOVERY_TEST_BOOT") {
      Ok(boot) => utils::operation::create_owner_with_boot_id(
        operation,
        "Vm",
        &key,
        incarnation,
        &node,
        &state_dir,
        &boot,
      ),
      Err(_) => utils::operation::create_owner(
        operation,
        "Vm",
        &key,
        incarnation,
        &node,
        &state_dir,
      ),
    }
    .unwrap();
    println!(
      "VM_RECOVERY_OWNER:{}",
      serde_json::to_string(&owner).unwrap()
    );
    std::io::stdout().flush().unwrap();
    // Only the parent fixture terminates this process. Its kernel locks remain
    // live after publishing the exact token and until that process exits.
    let _ = std::io::stdin().read_exact(&mut [0]);
  }

  // Exercise the production scan after an actual controller process exits,
  // using a unique state root and node. The development daemon is untouched.
  #[ntex::test]
  async fn vm_recovery_controller_exit_scans_current_and_previous_boot() {
    use crate::models::OperationOwner;
    use std::{path::PathBuf, process::Stdio};
    use tokio::io::{AsyncBufReadExt, BufReader};

    struct StateDirectory(PathBuf);
    impl Drop for StateDirectory {
      fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
      }
    }

    let system = crate::utils::tests::gen_default_test_system().await;
    for previous_boot in [false, true] {
      let root = StateDirectory(std::env::temp_dir().join(format!(
        "nanocl-recovery-controller-{}",
        uuid::Uuid::new_v4()
      )));
      std::fs::create_dir(&root.0).unwrap();
      let node = format!("recovery-controller-{}", uuid::Uuid::new_v4());
      let mut state = test_state(
        &system.state,
        &node,
        system.state.inner.pool.clone(),
        system.state.inner.docker_api.clone(),
      );
      Arc::get_mut(&mut state.inner).unwrap().config.state_dir =
        root.0.to_str().unwrap().to_owned();
      let vm = create_vm(&system).await;
      let key = vm.spec.vm_key.clone();
      let mut child: Option<tokio::process::Child> = None;
      let result = std::panic::AssertUnwindSafe(async {
        let incarnation = VmDb::read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap()
          .0
          .incarnation_key;
        let mutex = DistributedMutexDb::create_from(
          DistributedMutexPartial {
            action: "starting".into(),
            resource_kind: "Vm".into(),
            resource_key: key.clone(),
            node_key: node.clone(),
            acquired_by: "daemon".into(),
          },
          &state.inner.pool,
        )
        .await
        .unwrap();
        let mut command =
          tokio::process::Command::new(std::env::current_exe().unwrap());
        command
          .args([
            "--exact",
            "utils::container::vm::recovery::tests::vm_recovery_owner_process",
            "--ignored",
            "--nocapture",
          ])
          .env("NANOCL_RECOVERY_TEST_VM", &key)
          .env("NANOCL_RECOVERY_TEST_NODE", &node)
          .env("NANOCL_RECOVERY_TEST_STATE", &root.0)
          .env("NANOCL_RECOVERY_TEST_OPERATION", mutex.key.to_string())
          .env("NANOCL_RECOVERY_TEST_INCARNATION", incarnation.to_string())
          .stdin(Stdio::piped())
          .stdout(Stdio::piped())
          .kill_on_drop(true);
        let simulated_boot = uuid::Uuid::new_v4().to_string();
        if previous_boot {
          command.env("NANOCL_RECOVERY_TEST_BOOT", &simulated_boot);
        } else {
          command.env_remove("NANOCL_RECOVERY_TEST_BOOT");
        }
        child = Some(command.spawn().unwrap());
        let output = child.as_mut().unwrap().stdout.take().unwrap();
        let mut lines = BufReader::new(output).lines();
        let owner: OperationOwner =
          tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.next_line().await.unwrap() {
              if let Some((_, payload)) = line.split_once("VM_RECOVERY_OWNER:")
              {
                return serde_json::from_str(payload).unwrap();
              }
            }
            panic!(
              "child controller exited without publishing its ownership token"
            );
          })
          .await
          .expect("child controller did not publish its ownership token");
        assert_eq!(owner.key, mutex.key);
        assert_eq!(owner.incarnation_key, incarnation);
        assert_eq!(owner.node_name, node);
        assert_eq!(owner.resource_kind, "Vm");
        assert_eq!(owner.resource_key, key);
        if previous_boot {
          assert_eq!(owner.owner_boot_id, simulated_boot);
          assert_ne!(
            owner.owner_boot_id,
            std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
              .unwrap()
              .trim()
          );
        }
        let operation =
          OperationDb::new(&owner, "Vm", &key, vm.spec.key, "start");
        // The child owns both kernel locks. Commit the synthetic acceptance
        // without registering an executor, modeling that exact crash gap.
        let accepted = VmDb::accept_operation(
          &vm,
          None,
          operation.clone(),
          (),
          &state.inner.pool,
        )
        .await
        .unwrap()
        .vm;
        assert!(recover_one(&operation, &state).await.is_err());
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap(),
          Some(operation.clone())
        );
        assert_eq!(
          VmDb::transform_read_by_pk(&key, &state.inner.pool)
            .await
            .unwrap()
            .status,
          accepted.status
        );

        child.as_mut().unwrap().kill().await.unwrap();
        child.as_mut().unwrap().wait().await.unwrap();
        child = None;
        let owner_path = root
          .0
          .join("run/operations")
          .join(format!("{}.lock", operation.key));
        let hidden_owner = root.0.join("temporarily-missing-owner.lock");
        std::fs::rename(&owner_path, &hidden_owner).unwrap();
        let missing_result = recover_one(&operation, &state).await;
        std::fs::rename(&hidden_owner, &owner_path).unwrap();
        assert!(missing_result.is_err());
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap(),
          Some(operation.clone())
        );

        let unavailable = test_state(
          &state,
          &node,
          state.inner.pool.clone(),
          bollard_next::Docker::connect_with_unix(
            "/dev/null",
            1,
            bollard_next::API_DEFAULT_VERSION,
          )
          .unwrap(),
        );
        assert!(recover_one(&operation, &unavailable).await.is_err());
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap(),
          Some(operation.clone())
        );
        assert_eq!(
          VmDb::transform_read_by_pk(&key, &state.inner.pool)
            .await
            .unwrap()
            .status,
          accepted.status
        );
        assert!(owner_path.exists());

        // Use the same bounded pass as startup/background recovery. A failed
        // observation must leave the exact acceptance unchanged until retry.
        let mut settled = operation.clone();
        for _ in 0..3 {
          assert_eq!(recover_pending(&state, None).await.unwrap(), None);
          settled = OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap();
          if settled.outcome.is_some() {
            break;
          }
          assert_eq!(settled, operation);
          assert_eq!(
            VmDb::transform_read_by_pk(&key, &state.inner.pool)
              .await
              .unwrap()
              .status,
            accepted.status
          );
        }
        assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
        assert_eq!(settled.owner(), owner);
        assert_eq!(settled.spec_key, accepted.spec.key);
        assert_eq!(settled.diagnostic.as_ref().unwrap()["Runtime"], "absent");
        let observed = VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap();
        assert_eq!(observed.spec.key, accepted.spec.key);
        assert_eq!(observed.status.actual, ObjPsStatusKind::Stop);
        assert_eq!(observed.status.wanted, ObjPsStatusKind::Stop);
        assert!(!owner_path.exists());
        assert!(
          state
            .inner
            .task_manager
            .get_task(&format!("Vm@{key}"))
            .await
            .is_none()
        );
        assert!(matches!(
          state
            .inner
            .docker_api
            .inspect_container(&format!("{key}.v"), None)
            .await,
          Err(bollard_next::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
          })
        ));
        assert!(!recover_one(&operation, &state).await.unwrap());
        recover_pending(&state, None).await.unwrap();
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap(),
          Some(settled)
        );

        let (_, successor, guard) =
          accept(&observed, "starting", None, &state).await;
        assert!(
          VmDb::finish_operation(
            &successor,
            ObjPsStatusKind::Stop,
            "completed",
            json!({"Test": "completed same-spec successor"}),
            guard.clone(),
            &state.inner.pool
          )
          .await
          .unwrap()
        );
        utils::operation::cleanup_owner(&guard, &state.inner.pool)
          .await
          .unwrap();
        drop(guard);
        let completed = OperationDb::current("Vm", &key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap();
        assert_eq!(completed.key, successor.key);
        assert_eq!(completed.spec_key, operation.spec_key);
        assert_eq!(completed.outcome.as_deref(), Some("completed"));
        assert!(!recover_one(&operation, &state).await.unwrap());
        recover_pending(&state, None).await.unwrap();
        assert_eq!(
          OperationDb::current("Vm", &key, &state.inner.pool)
            .await
            .unwrap(),
          Some(completed)
        );
      })
      .catch_unwind()
      .await;
      if let Some(mut child) = child {
        let _ = child.kill().await;
        let _ = child.wait().await;
      }
      // No runtime is created by this fixture. Settle any exact pending test
      // acceptance before deleting its VM, preserving the original panic.
      let cleanup = async {
        if let Some(operation) =
          OperationDb::current("Vm", &key, &state.inner.pool).await?
          && operation.outcome.is_none()
        {
          let guard = utils::operation::abandoned_owner(
            &operation.owner(),
            &node,
            &state.inner.config.state_dir,
          )?
          .ok_or_else(|| {
            conflict("test owner remains live after child exit")
          })?;
          VmDb::finish_operation(
            &operation,
            ObjPsStatusKind::Stop,
            "interrupted",
            json!({"Test": "controller fixture cleanup"}),
            guard.clone(),
            &state.inner.pool,
          )
          .await?;
          utils::operation::cleanup_owner(&guard, &state.inner.pool).await?;
        }
        cleanup_mutex(&key, &state).await;
        VmDb::clear_by_pk(&key, &state.inner.pool).await?;
        Ok::<(), IoError>(())
      }
      .await;
      if let Err(panic) = result {
        std::panic::resume_unwind(panic);
      }
      cleanup.unwrap();
    }
  }
}
