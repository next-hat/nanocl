use diesel::prelude::*;
use nanocl_error::io::IoResult;
use nanocl_stubs::{
  system::{ObjPsHealthStatusKind, ObjPsStatusKind},
  vm::Vm,
};

use super::{
  generic::RepositoryReadByTransform,
  operation::{
    self, TransactionResult, conflict, run_query_owned, transaction,
  },
};
use crate::{
  models::{
    AcceptedVmOperation, DistributedMutexDb, ObjPsStatusDb, ObjPsStatusUpdate,
    OperationDb, Pool, SpecDb, VmDb,
  },
  schema::{
    distributed_mutexes, object_process_statuses, operations,
    resource_incarnations, specs, vms,
  },
};

fn pending_phase(phase: &str) -> bool {
  matches!(phase, "starting" | "updating" | "destroying")
}
fn phase(action: &str) -> TransactionResult<&'static str> {
  match action {
    "start" => Ok("starting"),
    "update" => Ok("updating"),
    "delete" => Ok("destroying"),
    _ => Err(conflict("Unsupported VM operation action")),
  }
}

fn accept_on_conn<F>(
  conn: &mut PgConnection,
  expected: &Vm,
  replacement: Option<&SpecDb>,
  operation: &OperationDb,
  write_pending: &mut F,
) -> TransactionResult<AcceptedVmOperation>
where
  F: FnMut(
    &mut PgConnection,
    &str,
    ObjPsStatusUpdate,
  ) -> diesel::QueryResult<ObjPsStatusDb>,
{
  // Identity is always first: ownership-file maintenance uses the same fence.
  let identity = operation::lock_incarnation(conn, operation.incarnation_key)?;
  let mut vm = vms::table
    .find(&expected.spec.vm_key)
    .for_update()
    .first::<VmDb>(conn)?;
  let status = object_process_statuses::table
    .find(&vm.status_key)
    .for_update()
    .first::<ObjPsStatusDb>(conn)?;
  if vm.incarnation_key != identity.key
    || vm.spec_key != expected.spec.key
    || operation.resource_kind != "Vm"
    || operation.resource_key != vm.key
    || operation.spec_key != replacement.map_or(vm.spec_key, |spec| spec.key)
  {
    return Err(conflict(
      "VM incarnation or specification changed during acceptance",
    ));
  }
  if pending_phase(&status.actual) {
    return Err(conflict("VM has an unresolved pending operation"));
  }
  let pending = phase(&operation.action)?;
  let mutex = distributed_mutexes::table
    .find(operation.key)
    .for_update()
    .first::<DistributedMutexDb>(conn)?;
  if mutex.resource_kind != "Vm"
    || mutex.resource_key != vm.key
    || mutex.node_key != operation.node_name
  {
    return Err(conflict("VM operation does not own its exact mutex"));
  }
  let spec = match replacement {
    Some(spec) => {
      if spec.kind_name != "Vm"
        || spec.kind_key != vm.key
        || spec
          .incarnation_key
          .is_some_and(|key| key != vm.incarnation_key)
      {
        return Err(conflict(
          "Replacement specification belongs to another incarnation",
        ));
      }
      let revision = spec.try_to_vm_spec()?;
      let mut spec = spec.clone();
      spec.incarnation_key = Some(vm.incarnation_key);
      let spec = diesel::insert_into(specs::table)
        .values(&spec)
        .get_result::<SpecDb>(conn)?;
      vm = diesel::update(vms::table.find(&vm.key))
        .set((vms::spec_key.eq(spec.key), vms::name.eq(revision.name)))
        .get_result(conn)?;
      spec
    }
    None => specs::table.find(vm.spec_key).first::<SpecDb>(conn)?,
  };
  if spec.incarnation_key != Some(vm.incarnation_key) {
    return Err(conflict("Accepted specification has another incarnation"));
  }
  operation::accept_on_conn(conn, &identity, operation)?;
  let wanted = if operation.action == "delete" {
    ObjPsStatusKind::Destroy
  } else {
    ObjPsStatusKind::Start
  };
  let status = write_pending(
    conn,
    &vm.status_key,
    ObjPsStatusUpdate {
      wanted: Some(wanted.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(pending.into()),
      prev_actual: Some(status.actual),
      health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
    },
  )?;
  Ok(AcceptedVmOperation {
    vm: VmDb::transform((vm, spec, status))?,
    operation: operation.clone(),
  })
}

impl VmDb {
  pub(crate) async fn accept_operation<G: Send + 'static>(
    expected: &Vm,
    replacement: Option<SpecDb>,
    operation: OperationDb,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<AcceptedVmOperation> {
    Self::accept_operation_with_pending_write(
      expected,
      replacement,
      operation,
      keepalive,
      pool,
      |conn, key, update| {
        diesel::update(object_process_statuses::table.find(key))
          .set(update)
          .get_result(conn)
      },
    )
    .await
  }

  pub(crate) async fn accept_operation_with_pending_write<F, G>(
    expected: &Vm,
    replacement: Option<SpecDb>,
    operation: OperationDb,
    keepalive: G,
    pool: &Pool,
    mut write_pending: F,
  ) -> IoResult<AcceptedVmOperation>
  where
    G: Send + 'static,
    F: FnMut(
        &mut PgConnection,
        &str,
        ObjPsStatusUpdate,
      ) -> diesel::QueryResult<ObjPsStatusDb>
      + Send
      + 'static,
  {
    let expected = expected.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        accept_on_conn(
          conn,
          &expected,
          replacement.as_ref(),
          &operation,
          &mut write_pending,
        )
      })
    })
    .await
  }

  pub(crate) async fn observe_runtime_status<G: Send + 'static>(
    expected: &Vm,
    expected_incarnation: uuid::Uuid,
    expected_operation: Option<uuid::Uuid>,
    actual: ObjPsStatusKind,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let expected = expected.clone();
    let actual = actual.to_string();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let identity = operation::lock_incarnation(conn, expected_incarnation)?;
        if identity.deleted_at.is_some()
          || identity.resource_kind != "Vm"
          || identity.resource_key != expected.spec.vm_key
          || identity.current_operation_key != expected_operation
        {
          return Ok(false);
        }
        let Some(vm) = vms::table
          .find(&expected.spec.vm_key)
          .for_update()
          .first::<VmDb>(conn)
          .optional()?
        else {
          return Ok(false);
        };
        let status = object_process_statuses::table
          .find(&vm.status_key)
          .for_update()
          .first::<ObjPsStatusDb>(conn)?;
        if let Some(key) = identity.current_operation_key
          && operations::table
            .find(key)
            .for_update()
            .first::<OperationDb>(conn)?
            .outcome
            .is_none()
        {
          return Ok(false);
        }
        if vm.incarnation_key != expected_incarnation
          || vm.spec_key != expected.spec.key
          || nanocl_stubs::system::ObjPsStatus::try_from(status.clone())?
            != expected.status
          || status.actual == actual
        {
          return Ok(false);
        }
        diesel::update(object_process_statuses::table.find(&vm.status_key))
          .set(ObjPsStatusUpdate {
            actual: Some(actual.clone()),
            prev_actual: Some(status.actual),
            ..Default::default()
          })
          .execute(conn)?;
        Ok(true)
      })
    })
    .await
  }

  pub(crate) async fn finish_operation<G: Send + 'static>(
    operation: &OperationDb,
    actual: ObjPsStatusKind,
    outcome: &str,
    diagnostic: serde_json::Value,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    let actual = actual.to_string();
    let outcome = outcome.to_owned();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        if pending_phase(&actual) {
          return Err(conflict(
            "Recovery must leave the pending runtime phase",
          ));
        }
        let Some(identity) = operation::lock_current(conn, &operation)? else {
          return Ok(false);
        };
        if identity.deleted_at.is_some() || operation.resource_kind != "Vm" {
          return Ok(false);
        }
        let Some(vm) = vms::table
          .find(&operation.resource_key)
          .for_update()
          .first::<VmDb>(conn)
          .optional()?
        else {
          return Ok(false);
        };
        if vm.incarnation_key != operation.incarnation_key
          || vm.spec_key != operation.spec_key
        {
          return Ok(false);
        }
        let status = object_process_statuses::table
          .find(&vm.status_key)
          .for_update()
          .first::<ObjPsStatusDb>(conn)?;
        diesel::update(object_process_statuses::table.find(&vm.status_key))
          .set(ObjPsStatusUpdate {
            wanted: Some(actual.clone()),
            prev_wanted: Some(status.wanted),
            actual: Some(actual.clone()),
            prev_actual: Some(status.actual),
            health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
          })
          .execute(conn)?;
        operation::complete_on_conn(
          conn,
          &operation,
          &outcome,
          Some(&diagnostic),
        )?;
        Ok(true)
      })
    })
    .await
  }

  pub(crate) async fn complete_operation<G: Send + 'static>(
    operation: &OperationDb,
    outcome: &str,
    diagnostic: Option<serde_json::Value>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    let outcome = outcome.to_owned();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(identity) = operation::lock_current(conn, &operation)? else {
          return Ok(false);
        };
        if identity.deleted_at.is_some() || operation.resource_kind != "Vm" {
          return Ok(false);
        }
        let Some(vm) = vms::table
          .find(&operation.resource_key)
          .for_update()
          .first::<VmDb>(conn)
          .optional()?
        else {
          return Ok(false);
        };
        if vm.incarnation_key != operation.incarnation_key
          || vm.spec_key != operation.spec_key
        {
          return Ok(false);
        }
        let status = object_process_statuses::table
          .find(&vm.status_key)
          .for_update()
          .first::<ObjPsStatusDb>(conn)?;
        if pending_phase(&status.actual) {
          return Ok(false);
        }
        operation::complete_on_conn(
          conn,
          &operation,
          &outcome,
          diagnostic.as_ref(),
        )?;
        Ok(true)
      })
    })
    .await
  }

  /// Runtime removal precedes this transaction. The exact delete operation,
  /// retained identity and live definition are settled together.
  pub(crate) async fn delete_operation<G: Send + 'static>(
    operation: &OperationDb,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(identity) = operation::lock_current(conn, &operation)? else {
          return Ok(false);
        };
        if identity.deleted_at.is_some()
          || operation.resource_kind != "Vm"
          || operation.action != "delete"
        {
          return Ok(false);
        }
        let Some(vm) = vms::table
          .find(&operation.resource_key)
          .for_update()
          .first::<VmDb>(conn)
          .optional()?
        else {
          return Ok(false);
        };
        if vm.incarnation_key != operation.incarnation_key
          || vm.spec_key != operation.spec_key
        {
          return Ok(false);
        }
        diesel::delete(vms::table.find(&vm.key)).execute(conn)?;
        diesel::delete(object_process_statuses::table.find(&vm.status_key))
          .execute(conn)?;
        diesel::update(resource_incarnations::table.find(identity.key))
          .set(
            resource_incarnations::deleted_at
              .eq(chrono::Utc::now().naive_utc()),
          )
          .execute(conn)?;
        operation::complete_on_conn(conn, &operation, "completed", None)?;
        Ok(true)
      })
    })
    .await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    models::{
      DistributedMutexPartial, NamespaceDb, OperationOwner,
      ResourceIncarnationDb, SystemState,
    },
    repositories::generic::{
      RepositoryCreate, RepositoryDelByPk, RepositoryReadBy, RepositoryUpdate,
    },
    utils::{
      self,
      tests::{TestSystem, gen_test_system},
    },
    vars,
  };
  use nanocl_stubs::{system::ObjPsStatusPartial, vm_spec::VmSpecPartial};

  fn no_routes(_: &mut ntex::web::ServiceConfig<SystemState>) {}

  struct Fixture {
    system: TestSystem,
    namespace: String,
    vm: Vm,
    incarnation: uuid::Uuid,
    mutexes: Vec<uuid::Uuid>,
  }

  fn stopped(key: &str) -> ObjPsStatusPartial {
    ObjPsStatusPartial {
      key: key.into(),
      wanted: ObjPsStatusKind::Stop,
      prev_wanted: ObjPsStatusKind::Create,
      actual: ObjPsStatusKind::Stop,
      prev_actual: ObjPsStatusKind::Create,
      health: ObjPsHealthStatusKind::Unknown,
    }
  }

  impl Fixture {
    async fn new() -> Self {
      let system = gen_test_system(no_routes, vars::VERSION).await;
      let pool = &system.state.inner.pool;
      let namespace =
        format!("vm-transaction-{}", uuid::Uuid::new_v4().simple());
      NamespaceDb::create_from(NamespaceDb::new(&namespace), pool)
        .await
        .unwrap();
      let key = format!("{namespace}.guest");
      let spec = SpecDb::try_from_vm_partial(
        &key,
        vars::VERSION,
        &VmSpecPartial {
          name: "guest".into(),
          image: "/transaction-test-original.qcow2".into(),
          ..Default::default()
        },
      )
      .unwrap();
      let vm = VmDb::create_with_spec(
        &key,
        "guest",
        &namespace,
        spec,
        stopped(&key),
        pool,
      )
      .await
      .unwrap();
      let incarnation = VmDb::read_by_pk(&key, pool)
        .await
        .unwrap()
        .0
        .incarnation_key;
      Self {
        system,
        namespace,
        vm,
        incarnation,
        mutexes: Vec::new(),
      }
    }

    fn pool(&self) -> &Pool {
      &self.system.state.inner.pool
    }

    fn replacement(&self) -> SpecDb {
      SpecDb::try_from_vm_partial(
        &self.vm.spec.vm_key,
        vars::VERSION,
        &VmSpecPartial {
          name: "guest".into(),
          image: "/transaction-test-replacement.qcow2".into(),
          ..Default::default()
        },
      )
      .unwrap()
    }

    async fn operation(
      &mut self,
      revision: uuid::Uuid,
      action: &str,
    ) -> OperationDb {
      let mutex = DistributedMutexDb::create_from(
        DistributedMutexPartial {
          action: action.into(),
          resource_kind: "Vm".into(),
          resource_key: self.vm.spec.vm_key.clone(),
          node_key: self.system.state.inner.config.hostname.clone(),
          acquired_by: "vm-transaction-test".into(),
        },
        self.pool(),
      )
      .await
      .unwrap();
      self.mutexes.push(mutex.key);
      OperationDb::new(
        &OperationOwner {
          key: mutex.key,
          resource_kind: "Vm".into(),
          resource_key: self.vm.spec.vm_key.clone(),
          incarnation_key: self.incarnation,
          node_name: mutex.node_key,
          owner_boot_id: "transaction-test-boot".into(),
          lock_device: 1,
          lock_inode: 2,
        },
        "Vm",
        &self.vm.spec.vm_key,
        revision,
        action,
      )
    }

    async fn current(&self) -> OperationDb {
      OperationDb::current("Vm", &self.vm.spec.vm_key, self.pool())
        .await
        .unwrap()
        .unwrap()
    }

    async fn settle(&self, operation: &OperationDb) {
      assert!(
        VmDb::finish_operation(
          operation,
          ObjPsStatusKind::Stop,
          "interrupted",
          serde_json::Value::Null,
          (),
          self.pool()
        )
        .await
        .unwrap()
      );
    }

    async fn cleanup(self) {
      if let Some(current) =
        OperationDb::current("Vm", &self.vm.spec.vm_key, self.pool())
          .await
          .unwrap()
        && current.outcome.is_none()
      {
        self.settle(&current).await;
      }
      for mutex in &self.mutexes {
        DistributedMutexDb::del_by_pk(mutex, self.pool())
          .await
          .unwrap();
      }
      if VmDb::read_by_pk(&self.vm.spec.vm_key, self.pool())
        .await
        .is_ok()
      {
        VmDb::clear_by_pk(&self.vm.spec.vm_key, self.pool())
          .await
          .unwrap();
      }
      NamespaceDb::del_by_pk(&self.namespace, self.pool())
        .await
        .unwrap();
      self.system.state.wait_event_loop().await;
    }
  }

  #[ntex::test]
  async fn vm_acceptance_rolls_back_spec_pointer_and_owner_when_pending_sql_fails()
   {
    let mut fixture = Fixture::new().await;
    let replacement = fixture.replacement();
    let revision = replacement.key;
    let operation = fixture.operation(revision, "update").await;
    let operation_key = operation.key;
    let incarnation = fixture.incarnation;
    let error = VmDb::accept_operation_with_pending_write(
      &fixture.vm,
      Some(replacement),
      operation,
      (),
      fixture.pool(),
      move |conn, key, update| {
        assert_eq!(
          vms::table
            .find(key)
            .select(vms::spec_key)
            .first::<uuid::Uuid>(conn)?,
          revision
        );
        assert_eq!(
          operations::table
            .find(operation_key)
            .select(operations::spec_key)
            .first::<uuid::Uuid>(conn)?,
          revision
        );
        assert_eq!(
          resource_incarnations::table
            .find(incarnation)
            .select(resource_incarnations::current_operation_key)
            .first::<Option<uuid::Uuid>>(conn)?,
          Some(operation_key)
        );
        assert_eq!(update.actual.as_deref(), Some("updating"));
        diesel::sql_query(
          "UPDATE object_process_statuses SET actual = NULL WHERE key = $1",
        )
        .bind::<diesel::sql_types::Text, _>(key)
        .execute(conn)?;
        unreachable!("actual is a NOT NULL column")
      },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("actual"), "{error}");
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.spec.key, fixture.vm.spec.key);
    assert_eq!(current.status, fixture.vm.status);
    assert_eq!(
      VmDb::history(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .len(),
      1
    );
    assert!(
      OperationDb::get(operation_key, fixture.pool())
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
      ResourceIncarnationDb::get(incarnation, fixture.pool())
        .await
        .unwrap()
        .unwrap()
        .current_operation_key,
      None
    );
    assert!(
      DistributedMutexDb::read_by_pk(&operation_key, fixture.pool())
        .await
        .is_ok()
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_acceptance_commits_fixed_revision_pending_status_and_owner() {
    let mut fixture = Fixture::new().await;
    let replacement = fixture.replacement();
    let revision = replacement.key;
    let operation = fixture.operation(revision, "update").await;
    let accepted = VmDb::accept_operation(
      &fixture.vm,
      Some(replacement),
      operation.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(accepted.vm.spec.key, revision);
    assert_eq!(
      accepted.vm.spec.image,
      "/transaction-test-replacement.qcow2"
    );
    assert_eq!(accepted.vm.status.actual, ObjPsStatusKind::Updating);
    assert_eq!(accepted.vm.status.prev_actual, ObjPsStatusKind::Stop);
    assert_eq!(accepted.vm.status.wanted, ObjPsStatusKind::Start);
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.spec.key, accepted.vm.spec.key);
    assert_eq!(current.status, accepted.vm.status);
    // Stored PostgreSQL timestamps and the returned accepted identity are exact.
    assert_eq!(accepted.operation, operation);
    assert_eq!(fixture.current().await, accepted.operation);
    assert_eq!(
      VmDb::history(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .len(),
      2
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_start_rejects_missing_mac_without_persisting_acceptance() {
    let mut fixture = Fixture::new().await;
    assert!(fixture.vm.spec.mac_address.is_none());
    let admission = fixture.operation(fixture.vm.spec.key, "start").await;
    let directory = std::env::temp_dir()
      .join(format!("nanocl-missing-mac-{}", uuid::Uuid::new_v4()));
    let (owner, guard) = utils::operation::create_owner(
      admission.key,
      "Vm",
      &fixture.vm.spec.vm_key,
      fixture.incarnation,
      &admission.node_name,
      directory.to_str().unwrap(),
    )
    .unwrap();
    let lifecycle = std::sync::Arc::new(
      utils::container::vm::lifecycle_lock(&fixture.vm.spec.vm_key)
        .lock_owned()
        .await,
    );
    let error = VmDb::prepare_start(
      &fixture.vm.spec.vm_key,
      owner,
      (lifecycle, guard.clone()),
      &fixture.system.state,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("MacAddress"), "{error}");
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.spec.key, fixture.vm.spec.key);
    assert_eq!(current.status, fixture.vm.status);
    assert!(current.spec.mac_address.is_none());
    assert_eq!(
      VmDb::history(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .len(),
      1
    );
    assert!(
      OperationDb::current("Vm", &fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      DistributedMutexDb::read_by_pk(&admission.key, fixture.pool())
        .await
        .is_ok()
    );
    assert!(
      utils::operation::cleanup_owner(&guard, fixture.pool())
        .await
        .unwrap()
    );
    drop(guard);
    std::fs::remove_dir_all(directory).unwrap();
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_recovery_and_completion_preserve_same_revision_successor() {
    let mut fixture = Fixture::new().await;
    let first = fixture.operation(fixture.vm.spec.key, "start").await;
    VmDb::accept_operation(
      &fixture.vm,
      None,
      first.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    fixture.settle(&first).await;
    let recovered =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    let successor = fixture.operation(recovered.spec.key, "start").await;
    VmDb::accept_operation(
      &recovered,
      None,
      successor.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      !VmDb::finish_operation(
        &first,
        ObjPsStatusKind::Start,
        "interrupted",
        serde_json::json!({"stale":true}),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      !VmDb::complete_operation(&first, "completed", None, (), fixture.pool())
        .await
        .unwrap()
    );
    assert_eq!(fixture.current().await, successor);
    assert!(
      DistributedMutexDb::read_by_pk(&successor.key, fixture.pool())
        .await
        .is_ok()
    );
    assert_eq!(
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .status
        .actual,
      ObjPsStatusKind::Starting
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_pending_owner_blocks_acceptance_after_mutex_collection() {
    let mut fixture = Fixture::new().await;
    let original = fixture.operation(fixture.vm.spec.key, "start").await;
    let accepted = VmDb::accept_operation(
      &fixture.vm,
      None,
      original.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    DistributedMutexDb::del_by_pk(&original.key, fixture.pool())
      .await
      .unwrap();
    let successor = fixture.operation(fixture.vm.spec.key, "start").await;
    let error = VmDb::accept_operation(
      &accepted.vm,
      None,
      successor.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("unresolved pending operation"));
    assert_eq!(fixture.current().await, original);
    assert!(
      DistributedMutexDb::read_by_pk(&successor.key, fixture.pool())
        .await
        .is_ok()
    );
    assert!(
      VmDb::clear_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .is_err()
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_observation_preserves_pending_and_completed_same_revision_successors()
   {
    let mut fixture = Fixture::new().await;
    let first = fixture.operation(fixture.vm.spec.key, "start").await;
    VmDb::accept_operation(
      &fixture.vm,
      None,
      first.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    fixture.settle(&first).await;
    let snapshot =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    let successor = fixture.operation(snapshot.spec.key, "start").await;
    VmDb::accept_operation(
      &snapshot,
      None,
      successor.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      !VmDb::observe_runtime_status(
        &snapshot,
        fixture.incarnation,
        Some(first.key),
        ObjPsStatusKind::Fail,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    fixture.settle(&successor).await;
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.spec.key, snapshot.spec.key);
    assert_eq!(
      current.status, snapshot.status,
      "completed successor restored identical status"
    );
    assert!(
      !VmDb::observe_runtime_status(
        &snapshot,
        fixture.incarnation,
        Some(first.key),
        ObjPsStatusKind::Fail,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      VmDb::observe_runtime_status(
        &current,
        fixture.incarnation,
        Some(successor.key),
        ObjPsStatusKind::Start,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert_eq!(fixture.current().await.key, successor.key);
    let history =
      OperationDb::history("Vm", fixture.incarnation, None, 1, fixture.pool())
        .await
        .unwrap();
    assert_eq!(history[0].key, first.key);
    let next = OperationDb::history(
      "Vm",
      fixture.incarnation,
      Some((history[0].accepted_at, history[0].key)),
      1,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(next[0].key, successor.key);
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_recovery_settles_crash_after_runtime_status_before_completion() {
    let mut fixture = Fixture::new().await;
    let operation = fixture.operation(fixture.vm.spec.key, "start").await;
    VmDb::accept_operation(
      &fixture.vm,
      None,
      operation.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    ObjPsStatusDb::update_pk(
      &fixture.vm.spec.vm_key,
      ObjPsStatusUpdate {
        actual: Some(ObjPsStatusKind::Start.to_string()),
        ..Default::default()
      },
      fixture.pool(),
    )
    .await
    .unwrap();
    let observation = serde_json::json!({"container_id":"owned-container","spec_key":operation.spec_key,"running":true});
    let error = utils::container::vm::runtime_resources(
      &fixture.vm.spec.vm_key,
      &fixture.system.state,
    )
    .await
    .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("awaiting completion or recovery")
    );
    assert_eq!(fixture.current().await, operation);
    assert!(
      VmDb::finish_operation(
        &operation,
        ObjPsStatusKind::Start,
        "interrupted",
        observation.clone(),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      !VmDb::finish_operation(
        &operation,
        ObjPsStatusKind::Stop,
        "interrupted",
        serde_json::Value::Null,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    let settled = fixture.current().await;
    assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
    assert_eq!(settled.diagnostic, Some(observation));
    assert!(
      DistributedMutexDb::read_by_pk(&operation.key, fixture.pool())
        .await
        .is_err()
    );
    let vm =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(vm.status.actual, ObjPsStatusKind::Start);
    assert_eq!(vm.status.wanted, ObjPsStatusKind::Start);
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_deletion_retains_history_and_recreation_has_a_new_incarnation() {
    let mut fixture = Fixture::new().await;
    let replacement = fixture.replacement();
    let accepted_revision = replacement.key;
    let update = fixture.operation(accepted_revision, "update").await;
    VmDb::accept_operation(
      &fixture.vm,
      Some(replacement),
      update.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    fixture.settle(&update).await;
    let before_delete =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    let deletion = fixture.operation(before_delete.spec.key, "delete").await;
    VmDb::accept_operation(
      &before_delete,
      None,
      deletion.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      VmDb::delete_operation(&deletion, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      VmDb::read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .is_err()
    );
    let identity =
      ResourceIncarnationDb::get(fixture.incarnation, fixture.pool())
        .await
        .unwrap()
        .unwrap();
    assert!(identity.deleted_at.is_some());
    assert_eq!(identity.current_operation_key, Some(deletion.key));
    let history = OperationDb::history(
      "Vm",
      fixture.incarnation,
      None,
      128,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].key, update.key);
    assert_eq!(history[1].key, deletion.key);
    assert_eq!(history[1].outcome.as_deref(), Some("completed"));
    let retained = SpecDb::read_by_pk(&accepted_revision, fixture.pool())
      .await
      .unwrap();
    assert_eq!(retained.incarnation_key, Some(fixture.incarnation));
    assert_eq!(
      retained.try_to_vm_spec().unwrap().image,
      "/transaction-test-replacement.qcow2"
    );
    assert_eq!(
      OperationDb::current_for_incarnation(fixture.incarnation, fixture.pool())
        .await
        .unwrap()
        .unwrap()
        .key,
      deletion.key
    );
    let new_spec = fixture.replacement();
    let recreated = VmDb::create_with_spec(
      &fixture.vm.spec.vm_key,
      "guest",
      &fixture.namespace,
      new_spec,
      stopped(&fixture.vm.spec.vm_key),
      fixture.pool(),
    )
    .await
    .unwrap();
    let new_incarnation =
      VmDb::read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .0
        .incarnation_key;
    assert_ne!(new_incarnation, fixture.incarnation);
    assert!(
      OperationDb::current("Vm", &fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
      VmDb::history(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .len(),
      1
    );
    assert!(
      OperationDb::history("Vm", new_incarnation, None, 128, fixture.pool())
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      !VmDb::delete_operation(&deletion, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      !VmDb::finish_operation(
        &update,
        ObjPsStatusKind::Fail,
        "failed",
        serde_json::Value::Null,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      !VmDb::observe_runtime_status(
        &before_delete,
        fixture.incarnation,
        Some(deletion.key),
        ObjPsStatusKind::Fail,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.spec.key, recreated.spec.key);
    assert_eq!(current.status, recreated.status);
    fixture.incarnation = new_incarnation;
    fixture.cleanup().await;
  }
}
