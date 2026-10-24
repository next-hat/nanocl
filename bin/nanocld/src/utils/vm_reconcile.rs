//! Offline, explicitly quiescent VM reconciliation. No task or runtime action
//! is dispatched, and inspection never prints specification contents/secrets.
use std::{sync::Arc, time::Duration};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{config::DaemonConfig, system::ObjPsStatusKind};
use serde_json::{Value, json};

use crate::{
  models::{
    OperationDb, RawEventEmitter, ResourceIncarnationDb, StateDirectoryGuard,
    SystemState, SystemStateInner, TaskManager, VmDb, VmReconcileArgs,
  },
  repositories::generic::*,
};

fn conflict(message: &str) -> IoError {
  IoError::invalid_data("Offline VM reconciliation", message)
}

fn validate(args: &VmReconcileArgs) -> IoResult<()> {
  if args.confirm_quiescent
    && (args.incarnation.is_none()
      || args.spec.is_none()
      || (args.operation.is_some() == args.legacy))
  {
    return Err(conflict(
      "Inspect first; reconciliation requires --incarnation, --spec and exactly one of --operation or --legacy, plus --confirm-quiescent",
    ));
  }
  Ok(())
}

/// Connect only to existing dependencies. In particular, do not call
/// SystemState::new/init: those run migrations, event consumers and runtime
/// inventory. A disconnected event sender is intentional in this command.
async fn connect(config: &DaemonConfig) -> IoResult<SystemState> {
  let address = config.store_addr.as_deref().ok_or_else(|| {
    conflict("Supply --store-addr for the existing database; this command never initializes a store")
  })?;
  let pool = super::store::create_pool(address).await?;
  let docker_api = bollard_next::Docker::connect_with_unix(
    &config.docker_host,
    5,
    bollard_next::API_DEFAULT_VERSION,
  )
  .map_err(|error| conflict(&format!("Docker connection: {error}")))?;
  let (event_emitter, receiver) = futures::channel::mpsc::unbounded();
  drop(receiver);
  Ok(SystemState {
    inner: Arc::new(SystemStateInner {
      pool,
      docker_api,
      config: config.clone(),
      task_manager: TaskManager::default(),
      event_emitter,
      event_emitter_raw: RawEventEmitter::default(),
      arbiter: ntex::rt::Arbiter::current(),
    }),
  })
}

pub(crate) async fn run(
  args: &VmReconcileArgs,
  config: &DaemonConfig,
) -> IoResult<Value> {
  let gate = ownership_gate(args, config)?;
  let state = connect(config).await?;
  execute(args, &state, gate).await
}

fn ownership_gate(
  args: &VmReconcileArgs,
  config: &DaemonConfig,
) -> IoResult<StateDirectoryGuard> {
  validate(args)?;
  if args.confirm_quiescent {
    super::operation::acquire_reconciliation_guard(&config.state_dir)
  } else {
    super::operation::acquire_inspection_guard(&config.state_dir)
  }
}

async fn execute(
  args: &VmReconcileArgs,
  state: &SystemState,
  gate: StateDirectoryGuard,
) -> IoResult<Value> {
  let config = &state.inner.config;
  let (row, _, _) = VmDb::read_by_pk(&args.vm_key, &state.inner.pool).await?;
  let vm = VmDb::transform_read_by_pk(&args.vm_key, &state.inner.pool).await?;
  let identity =
    ResourceIncarnationDb::get(row.incarnation_key, &state.inner.pool)
      .await?
      .ok_or_else(|| {
        conflict("VM incarnation is missing; no reconciliation was performed")
      })?;
  let current = OperationDb::current_for_incarnation(
    row.incarnation_key,
    &state.inner.pool,
  )
  .await?;
  let mut report = json!({
    "ResourceKind": "Vm",
    "ResourceKey": row.key,
    "Incarnation": row.incarnation_key,
    "Specification": vm.spec.key,
    "CurrentOperation": identity.current_operation_key,
    "RecordedStatus": vm.status,
    "Operation": current.as_ref().map(|operation| json!({
      "Key": operation.key,
      "Action": operation.action,
      "Specification": operation.spec_key,
      "OwnerNode": operation.node_name,
      "OwnerBoot": operation.owner_boot_id,
      "AcceptedAt": operation.accepted_at,
      "CompletedAt": operation.completed_at,
      "Outcome": operation.outcome,
      "DiagnosticsAvailable": operation.diagnostic.is_some(),
      "RuntimeObservation": safe_observation(operation.diagnostic.as_ref()),
    })),
    "Reconciled": false,
    "Prerequisite": "Stop every daemon and operation owner using this database/runtime, including older binaries and alternate/copied state directories. Keep the database and Docker available. Do not remove active ownership files. Inspect auxiliary provisioning containers; this command never stops them.",
  });
  if !args.confirm_quiescent {
    report["AutomaticRecovery"] = match current.as_ref() {
      Some(operation) if operation.outcome.is_none() => {
        match super::operation::abandoned_owner(
          &operation.owner(),
          &config.hostname,
          &config.state_dir,
        ) {
          Ok(Some(_guard)) => {
            json!({"Ownership": "abandoned", "NextStep": "The scanner may reconcile after definitive runtime inspection; this inspection is not a takeover authorization."})
          }
          Ok(None) => {
            json!({"Ownership": "live", "NextStep": "Wait for the current owner; do not remove its file."})
          }
          Err(error) => {
            json!({"Ownership": "ambiguous", "Reason": error.to_string(), "NextStep": "Use exact offline reconciliation only after satisfying Prerequisite."})
          }
        }
      }
      None
        if matches!(
          vm.status.actual,
          ObjPsStatusKind::Starting
            | ObjPsStatusKind::Updating
            | ObjPsStatusKind::Destroying
        ) =>
      {
        json!({"Ownership": "legacy-unknown", "NextStep": "Use --legacy with exact incarnation/specification and confirmed quiescence."})
      }
      _ => json!({"Ownership": "not-pending"}),
    };
    report["NextStep"] = json!(
      "Repeat with the exact --incarnation, --spec and --operation shown above (or --legacy when CurrentOperation is null), and --confirm-quiescent only after satisfying Prerequisite."
    );
    return Ok(report);
  }
  if args.incarnation != Some(row.incarnation_key)
    || args.spec != Some(vm.spec.key)
    || identity.resource_kind != "Vm"
    || identity.resource_key != args.vm_key
    || identity.deleted_at.is_some()
  {
    return Err(conflict(
      "Incarnation or specification changed; inspect again before reconciling",
    ));
  }
  let lifecycle = Arc::new(
    super::container::vm::lifecycle_lock(&args.vm_key)
      .try_lock_owned()
      .await
      .map_err(|_| conflict("VM lifecycle is busy"))?,
  );
  let (operation, owner_guard) = match current {
    Some(operation) => {
      if args.legacy
        && operation.action == "reconcile"
        && operation.outcome.is_some()
        && operation.incarnation_key == row.incarnation_key
        && operation.spec_key == vm.spec.key
      {
        report["AlreadyTerminal"] = json!(true);
        return Ok(report);
      }
      if args.legacy
        || args.operation != Some(operation.key)
        || operation.incarnation_key != row.incarnation_key
        || operation.spec_key != vm.spec.key
      {
        return Err(conflict(
          "Current operation changed; historical UUIDs do not authorize reconciliation",
        ));
      }
      if operation.outcome.is_some() {
        report["AlreadyTerminal"] = json!(true);
        return Ok(report);
      }
      let owner_guard = super::operation::reconciliation_owner(
        &operation.owner(),
        &config.hostname,
        &gate,
      )?;
      (operation, owner_guard)
    }
    None => {
      if !args.legacy
        || args.operation.is_some()
        || identity.current_operation_key.is_some()
        || !matches!(
          vm.status.actual,
          ObjPsStatusKind::Starting
            | ObjPsStatusKind::Updating
            | ObjPsStatusKind::Destroying
        )
      {
        return Err(conflict(
          "--legacy requires an exact pending VM with no current operation; no prior owner is inferred",
        ));
      }
      let (owner, guard) = super::operation::create_owner_with_gate(
        uuid::Uuid::new_v4(),
        "Vm",
        &args.vm_key,
        row.incarnation_key,
        &config.hostname,
        gate.clone(),
      )?;
      (
        OperationDb::new(&owner, "Vm", &args.vm_key, vm.spec.key, "reconcile"),
        Some(guard),
      )
    }
  };
  let observed = tokio::time::timeout(Duration::from_secs(5), super::container::vm::recovery::observe(&vm, state)).await
    .map_err(|_| conflict("Runtime inspection timed out; accepted state and ownership evidence were preserved"))
    .and_then(|result| result);
  let (actual, mut observation) = match observed {
    Ok(observation) => observation,
    Err(error) => {
      if args.legacy
        && let Some(guard) = &owner_guard
        && let Err(cleanup) =
          super::operation::cleanup_owner(guard, &state.inner.pool).await
      {
        log::warn!(
          "Rejected legacy reconciliation cleanup deferred: {cleanup}"
        );
      }
      return Err(error);
    }
  };
  observation["Reconciliation"] = json!({
    "Mode": "offline-operator",
    "ConfirmedQuiescent": true,
    "OperatorNode": config.hostname,
    "LegacyOwnershipUnknown": args.legacy,
    "Note": "The operator confirmed all potential owners are stopped. The exclusive state-directory gate excludes compatible daemons. No runtime action was replayed.",
  });
  observation["Failure"] = json!(if args.legacy {
    "Legacy transition reconciled; original request ownership and outcome are unknown"
  } else {
    "Accepted request interrupted; observed runtime reconciled after operator-confirmed quiescence"
  });
  let keepalive = (gate.clone(), lifecycle, owner_guard.clone());
  let changed = if args.legacy {
    VmDb::reconcile_legacy(
      &vm,
      row.incarnation_key,
      operation.clone(),
      actual.clone(),
      observation,
      keepalive,
      &state.inner.pool,
    )
    .await?
  } else {
    VmDb::finish_operation(
      &operation,
      actual.clone(),
      "interrupted",
      observation,
      keepalive,
      &state.inner.pool,
    )
    .await?
  };
  if !changed {
    return Err(conflict(
      "Target changed during reconciliation; no successor was modified",
    ));
  }
  if let Some(guard) = &owner_guard
    && let Err(error) =
      super::operation::cleanup_owner(guard, &state.inner.pool).await
  {
    log::warn!(
      "Reconciliation completed; owner-file cleanup deferred: {error}"
    );
  }
  report["Reconciled"] = json!(true);
  report["ReconciledOperation"] = json!(operation.key);
  report["OperationOutcome"] = json!("interrupted");
  report["ObservedStatus"] = json!(actual);
  report["NextStep"] = json!(
    "Inspect the retained desired revision and runtime, restart the daemon, then explicitly retry the desired operation."
  );
  Ok(report)
}

// Diagnostics can contain arbitrary consumer errors. Only typed runtime facts
// are printed; accepted specs and free-form failure payloads stay out of CLI output.
fn safe_observation(diagnostic: Option<&Value>) -> Value {
  let Some(diagnostic) = diagnostic else {
    return Value::Null;
  };
  let mut output = serde_json::Map::new();
  if let Some(runtime) = diagnostic.get("Runtime").and_then(Value::as_str)
    && matches!(runtime, "absent" | "running" | "stopped")
  {
    output.insert("Runtime".into(), json!(runtime));
  }
  for name in ["Specification", "AcceptedSpecification"] {
    if let Some(value) = diagnostic
      .get(name)
      .and_then(Value::as_str)
      .and_then(|value| uuid::Uuid::parse_str(value).ok())
    {
      output.insert(name.into(), json!(value));
    }
  }
  if let Some(id) = diagnostic.get("ContainerId").and_then(Value::as_str)
    && id.len() == 64
    && id.bytes().all(|byte| byte.is_ascii_hexdigit())
  {
    output.insert("ContainerId".into(), json!(id));
  }
  if let Some(started) = diagnostic
    .get("StartedAt")
    .and_then(Value::as_str)
    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
  {
    output.insert("StartedAt".into(), json!(started.to_rfc3339()));
  }
  if let Some(observed) = diagnostic
    .get("AcceptedGenerationObserved")
    .and_then(Value::as_bool)
  {
    output.insert("AcceptedGenerationObserved".into(), json!(observed));
  }
  Value::Object(output)
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::Parser;

  #[test]
  fn vm_reconcile_requires_exact_target_and_explicit_quiescence() {
    let cli = crate::cli::Cli::parse_from([
      "nanocld",
      "vm-reconcile",
      "--vm-key",
      "global.example",
    ]);
    let Some(crate::models::DaemonCommand::VmReconcile(mut args)) = cli.command
    else {
      panic!("command missing")
    };
    assert!(validate(&args).is_ok());
    args.confirm_quiescent = true;
    assert!(validate(&args).is_err());
    args.incarnation = Some(uuid::Uuid::new_v4());
    args.spec = Some(uuid::Uuid::new_v4());
    assert!(validate(&args).is_err());
    args.operation = Some(uuid::Uuid::new_v4());
    assert!(validate(&args).is_ok());
    args.legacy = true;
    assert!(validate(&args).is_err());
    args.operation = None;
    assert!(validate(&args).is_ok());
  }

  #[test]
  fn vm_reconcile_report_excludes_free_form_diagnostics_and_specs() {
    let diagnostic = json!({
      "Runtime": "running", "ContainerId": "a".repeat(64),
      "Specification": uuid::Uuid::new_v4(), "StartedAt": "2026-10-24T12:00:00Z",
      "AcceptedGenerationObserved": false,
      "Failure": "private-password-marker", "SpecificationContents": {"Password": "private-password-marker"},
    });
    let report = safe_observation(Some(&diagnostic));
    assert_eq!(report["Runtime"], "running");
    assert_eq!(report["AcceptedGenerationObserved"], false);
    assert!(!report.to_string().contains("private-password-marker"));
    assert!(!report.to_string().contains("Failure"));
  }

  struct FixtureDirectory(std::path::PathBuf);

  impl FixtureDirectory {
    fn new() -> Self {
      let path = std::env::temp_dir()
        .join(format!("nanocl-offline-reconcile-{}", uuid::Uuid::new_v4()));
      std::fs::create_dir(&path).unwrap();
      Self(path)
    }
  }

  impl Drop for FixtureDirectory {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  fn fixture_state(
    system: &SystemState,
    directory: &FixtureDirectory,
  ) -> SystemState {
    let mut config = system.inner.config.clone();
    config.hostname = format!("offline-reconcile-{}", uuid::Uuid::new_v4());
    config.state_dir = directory.0.to_str().unwrap().to_owned();
    SystemState {
      inner: Arc::new(SystemStateInner {
        pool: system.inner.pool.clone(),
        docker_api: system.inner.docker_api.clone(),
        config,
        task_manager: Default::default(),
        event_emitter: system.inner.event_emitter.clone(),
        event_emitter_raw: Default::default(),
        arbiter: ntex::rt::Arbiter::current(),
      }),
    }
  }

  async fn create_fixture(state: &SystemState) -> nanocl_stubs::vm::Vm {
    use crate::objects::generic::ObjCreate;
    VmDb::fn_create_obj(
      &crate::models::VmObjCreateIn {
        namespace: "global".into(),
        version: crate::vars::VERSION.into(),
        spec: nanocl_stubs::vm_spec::VmSpecPartial {
          name: format!("offline-reconcile-{}", uuid::Uuid::new_v4().simple()),
          image: "/dev/null/offline-reconcile.img".into(),
          cloud_init: Some(false),
          password: Some("private-password-marker".into()),
          ..Default::default()
        },
      },
      state,
    )
    .await
    .unwrap()
  }

  async fn accept_fixture(
    vm: &nanocl_stubs::vm::Vm,
    state: &SystemState,
    old_boot: bool,
  ) -> (
    nanocl_stubs::vm::Vm,
    OperationDb,
    crate::models::VmOperationLease,
  ) {
    use crate::models::{DistributedMutexDb, DistributedMutexPartial};
    let mutex = DistributedMutexDb::create_from(
      DistributedMutexPartial {
        resource_kind: "Vm".into(),
        resource_key: vm.spec.vm_key.clone(),
        node_key: state.inner.config.hostname.clone(),
        action: "update".into(),
        acquired_by: "offline-test".into(),
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
    let (row, _, _) = VmDb::read_by_pk(&vm.spec.vm_key, &state.inner.pool)
      .await
      .unwrap();
    let (owner, guard) = crate::utils::operation::create_owner(
      mutex.key,
      "Vm",
      &vm.spec.vm_key,
      row.incarnation_key,
      &state.inner.config.hostname,
      &state.inner.config.state_dir,
    )
    .unwrap();
    let lifecycle = Arc::new(
      crate::utils::container::vm::lifecycle_lock(&vm.spec.vm_key)
        .try_lock_owned()
        .await
        .unwrap(),
    );
    let lease = (lifecycle, guard);
    let mut operation =
      OperationDb::new(&owner, "Vm", &vm.spec.vm_key, vm.spec.key, "update");
    if old_boot {
      operation.owner_boot_id = uuid::Uuid::new_v4().to_string();
    }
    let accepted = VmDb::accept_operation(
      vm,
      None,
      operation,
      lease.clone(),
      &state.inner.pool,
    )
    .await
    .unwrap();
    (accepted.vm, accepted.operation, lease)
  }

  fn exact_args(
    vm: &nanocl_stubs::vm::Vm,
    operation: &OperationDb,
  ) -> VmReconcileArgs {
    VmReconcileArgs {
      vm_key: vm.spec.vm_key.clone(),
      incarnation: Some(operation.incarnation_key),
      operation: Some(operation.key),
      legacy: false,
      spec: Some(vm.spec.key),
      confirm_quiescent: true,
    }
  }

  async fn command(
    args: &VmReconcileArgs,
    state: &SystemState,
  ) -> IoResult<Value> {
    let gate = ownership_gate(args, &state.inner.config)?;
    execute(args, state, gate).await
  }

  async fn assert_pending(
    vm: &nanocl_stubs::vm::Vm,
    operation: &OperationDb,
    state: &SystemState,
  ) {
    assert_eq!(
      OperationDb::get(operation.key, &state.inner.pool)
        .await
        .unwrap()
        .as_ref(),
      Some(operation)
    );
    let current =
      VmDb::transform_read_by_pk(&vm.spec.vm_key, &state.inner.pool)
        .await
        .unwrap();
    assert_eq!(current.spec.key, vm.spec.key);
    assert_eq!(current.status, vm.status);
  }

  async fn cleanup_fixture(key: &str, state: &SystemState) {
    if let Some(operation) = OperationDb::current("Vm", key, &state.inner.pool)
      .await
      .unwrap()
      && operation.outcome.is_none()
    {
      VmDb::finish_operation(
        &operation,
        ObjPsStatusKind::Stop,
        "interrupted",
        json!({"TestCleanup": true}),
        (),
        &state.inner.pool,
      )
      .await
      .unwrap();
    }
    VmDb::clear_by_pk(key, &state.inner.pool).await.unwrap();
  }

  // Uses the real database and Docker, with an isolated fixture node/directory.
  // The normal daemon cannot own or scan this VM's test operation. This models
  // missing/old-boot evidence; it is not a host reboot or daemon restart test.
  #[ntex::test]
  async fn vm_reconcile_offline_missing_and_old_boot_evidence_is_exact_and_idempotent()
   {
    use futures::FutureExt;
    let system = crate::utils::tests::gen_default_test_system().await;
    let directory = FixtureDirectory::new();
    let state = fixture_state(&system.state, &directory);
    for old_boot in [false, true] {
      let vm = create_fixture(&state).await;
      let result = std::panic::AssertUnwindSafe(async {
        let (accepted, operation, lease) =
          accept_fixture(&vm, &state, old_boot).await;
        let args = exact_args(&accepted, &operation);
        let live = command(&args, &state).await.unwrap_err();
        assert_eq!(live.inner.kind(), std::io::ErrorKind::WouldBlock);
        assert_pending(&accepted, &operation, &state).await;
        drop(lease);
        let path = directory
          .0
          .join("run/operations")
          .join(format!("{}.lock", operation.key));
        if !old_boot {
          std::fs::remove_file(&path).unwrap();
        }
        assert!(
          crate::utils::container::vm::recovery::recover_one(
            &operation, &state
          )
          .await
          .is_err()
        );
        let mut inspect = args.clone();
        inspect.confirm_quiescent = false;
        let report = command(&inspect, &state).await.unwrap();
        assert_eq!(report["AutomaticRecovery"]["Ownership"], "ambiguous");
        assert!(!report.to_string().contains("private-password-marker"));
        assert_pending(&accepted, &operation, &state).await;
        for field in ["incarnation", "operation", "spec"] {
          let mut stale = args.clone();
          match field {
            "incarnation" => stale.incarnation = Some(uuid::Uuid::new_v4()),
            "operation" => stale.operation = Some(uuid::Uuid::new_v4()),
            "spec" => stale.spec = Some(uuid::Uuid::new_v4()),
            _ => unreachable!(),
          }
          assert!(command(&stale, &state).await.is_err(), "{field}");
          assert_pending(&accepted, &operation, &state).await;
        }
        let mut unavailable = state.inner.config.clone();
        unavailable.docker_host = "/dev/null".into();
        assert!(run(&args, &unavailable).await.is_err());
        assert_pending(&accepted, &operation, &state).await;
        // Exercise the production connection + command entry point as well as
        // the exact reconciliation body used by the bounded dependency tests.
        let report = run(&args, &state.inner.config).await.unwrap();
        assert_eq!(report["Reconciled"], true);
        assert_eq!(report["ObservedStatus"], json!(ObjPsStatusKind::Stop));
        let terminal = OperationDb::get(operation.key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap();
        assert_eq!(terminal.outcome.as_deref(), Some("interrupted"));
        assert_eq!(terminal.diagnostic.as_ref().unwrap()["Runtime"], "absent");
        let observed =
          VmDb::transform_read_by_pk(&args.vm_key, &state.inner.pool)
            .await
            .unwrap();
        assert_eq!(observed.status.actual, ObjPsStatusKind::Stop);
        assert_eq!(observed.status.wanted, ObjPsStatusKind::Stop);
        assert_eq!(observed.spec.key, accepted.spec.key);
        assert!(
          state
            .inner
            .task_manager
            .get_task(&format!("Vm@{}", args.vm_key))
            .await
            .is_none()
        );
        assert_eq!(
          command(&args, &state).await.unwrap()["AlreadyTerminal"],
          true
        );
        assert_eq!(
          OperationDb::get(operation.key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          terminal
        );
        if !old_boot {
          let (next_vm, successor, next_lease) =
            accept_fixture(&observed, &state, false).await;
          drop(next_lease);
          assert!(command(&args, &state).await.is_err());
          assert_pending(&next_vm, &successor, &state).await;
          command(&exact_args(&next_vm, &successor), &state)
            .await
            .unwrap();
          let completed_successor =
            OperationDb::get(successor.key, &state.inner.pool)
              .await
              .unwrap()
              .unwrap();
          assert!(command(&args, &state).await.is_err());
          assert_eq!(
            OperationDb::get(successor.key, &state.inner.pool)
              .await
              .unwrap()
              .unwrap(),
            completed_successor
          );
          assert_eq!(
            OperationDb::get(operation.key, &state.inner.pool)
              .await
              .unwrap()
              .unwrap(),
            terminal
          );
        }
        if old_boot {
          assert!(
            path.exists(),
            "ambiguous header is preserved, not unlinked after reconciliation"
          );
        }
      })
      .catch_unwind()
      .await;
      cleanup_fixture(&vm.spec.vm_key, &state).await;
      if let Err(panic) = result {
        std::panic::resume_unwind(panic);
      }
    }
  }

  #[ntex::test]
  async fn vm_reconcile_legacy_requires_quiescence_and_preserves_failed_dependencies()
   {
    use crate::models::{ObjPsStatusDb, Pool};
    use diesel::{PgConnection, r2d2::ConnectionManager};
    use futures::FutureExt;
    let system = crate::utils::tests::gen_default_test_system().await;
    let directory = FixtureDirectory::new();
    let state = fixture_state(&system.state, &directory);
    let vm = create_fixture(&state).await;
    let result = std::panic::AssertUnwindSafe(async {
      let (row, _, _) = VmDb::read_by_pk(&vm.spec.vm_key, &state.inner.pool).await.unwrap();
      ObjPsStatusDb::update_actual_status(&vm.spec.vm_key, &ObjPsStatusKind::Updating, &state.inner.pool).await.unwrap();
      let before = VmDb::transform_read_by_pk(&vm.spec.vm_key, &state.inner.pool).await.unwrap();
      let args = VmReconcileArgs { vm_key: vm.spec.vm_key.clone(), incarnation: Some(row.incarnation_key), operation: None,
        legacy: true, spec: Some(vm.spec.key), confirm_quiescent: true };
      let daemon = crate::utils::operation::acquire_daemon_guard(&state.inner.config.state_dir).unwrap();
      assert_eq!(command(&args, &state).await.unwrap_err().inner.kind(), std::io::ErrorKind::WouldBlock);
      drop(daemon);
      let mut docker_unavailable = state.inner.config.clone();
      docker_unavailable.docker_host = "/dev/null".into();
      assert!(run(&args, &docker_unavailable).await.is_err());
      assert_eq!(std::fs::read_dir(directory.0.join("run/operations")).unwrap().count(), 0,
        "definitively rejected legacy attempts clean their owner files");
      let unavailable_pool = Pool::builder().min_idle(Some(0)).max_size(1).connection_timeout(Duration::from_millis(50))
        .build_unchecked(ConnectionManager::<PgConnection>::new("invalid-reconcile-test-connection-string"));
      let unavailable = SystemState { inner: Arc::new(SystemStateInner {
        pool: unavailable_pool, docker_api: state.inner.docker_api.clone(), config: state.inner.config.clone(), task_manager: Default::default(),
        event_emitter: state.inner.event_emitter.clone(), event_emitter_raw: Default::default(), arbiter: ntex::rt::Arbiter::current(),
      }) };
      assert!(command(&args, &unavailable).await.is_err());
      assert!(OperationDb::current("Vm", &args.vm_key, &state.inner.pool).await.unwrap().is_none());
      assert_eq!(VmDb::transform_read_by_pk(&args.vm_key, &state.inner.pool).await.unwrap().status, before.status);
      let report = command(&args, &state).await.unwrap();
      assert_eq!(report["Reconciled"], true);
      let terminal = OperationDb::current("Vm", &args.vm_key, &state.inner.pool).await.unwrap().unwrap();
      assert_eq!(terminal.action, "reconcile");
      assert_eq!(terminal.outcome.as_deref(), Some("interrupted"));
      assert_eq!(terminal.diagnostic.as_ref().unwrap()["Reconciliation"]["LegacyOwnershipUnknown"], true);
      assert_eq!(command(&args, &state).await.unwrap()["AlreadyTerminal"], true);
      assert_eq!(OperationDb::current("Vm", &args.vm_key, &state.inner.pool).await.unwrap().unwrap(), terminal);
      assert_eq!(VmDb::transform_read_by_pk(&args.vm_key, &state.inner.pool).await.unwrap().status.actual, ObjPsStatusKind::Stop);
      let path = directory.0.join("run/operations").join(format!("{}.lock", terminal.key));
      assert!(!path.exists(), "verified legacy reconciliation owner is cleaned after durable completion");
    }).catch_unwind().await;
    cleanup_fixture(&vm.spec.vm_key, &state).await;
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }
}
