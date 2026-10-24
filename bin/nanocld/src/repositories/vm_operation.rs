use diesel::prelude::*;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  system::{ObjPsHealthStatusKind, ObjPsStatusKind},
  vm::Vm,
};

use crate::{
  models::{
    DistributedMutexDb, ObjPsStatusDb, ObjPsStatusUpdate, Pool, SpecDb, VmDb,
    VmOperationDb, VmOperationTransactionError,
  },
  schema::{
    distributed_mutexes, object_process_statuses, specs, vm_operations, vms,
  },
  utils,
};

use super::generic::RepositoryReadByTransform;

type TransactionResult<T> = Result<T, VmOperationTransactionError>;

async fn run_query<T, F>(pool: &Pool, query: F) -> IoResult<T>
where
  T: Send + 'static,
  F: FnOnce(&mut PgConnection) -> IoResult<T> + Send + 'static,
{
  let pool = pool.clone();
  ntex::rt::spawn_blocking(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    query(&mut conn)
  })
  .await
  .map_err(|error| {
    IoError::interrupted("VM operation database query", &error.to_string())
  })?
}

// CockroachDB may abort a serializable transaction even with row locks. Only
// retry database serialization failures; these closures contain no runtime,
// executor, event or freshly generated operation/specification identities.
fn transaction<T>(
  conn: &mut PgConnection,
  mut query: impl FnMut(&mut PgConnection) -> TransactionResult<T>,
) -> IoResult<T> {
  for attempt in 0..3 {
    let result = conn
      .build_transaction()
      .serializable()
      .run::<_, VmOperationTransactionError, _>(&mut query);
    match result {
      Err(VmOperationTransactionError::Database(
        diesel::result::Error::DatabaseError(
          diesel::result::DatabaseErrorKind::SerializationFailure,
          _,
        ),
      )) if attempt < 2 => continue,
      result => return result.map_err(Into::into),
    }
  }
  unreachable!("bounded transaction loop returns its last result")
}

fn pending_phase(phase: &str) -> bool {
  matches!(phase, "starting" | "updating" | "destroying")
}

fn conflict(message: &str) -> VmOperationTransactionError {
  IoError::with_context(
    "VM operation acceptance",
    std::io::Error::new(std::io::ErrorKind::WouldBlock, message),
  )
  .into()
}

fn same_operation(current: &VmOperationDb, expected: &VmOperationDb) -> bool {
  current.key == expected.key
    && current.vm_key == expected.vm_key
    && current.spec_key == expected.spec_key
    && current.phase == expected.phase
    && current.node_name == expected.node_name
    && current.owner_boot_id == expected.owner_boot_id
    && current.lock_device == expected.lock_device
    && current.lock_inode == expected.lock_inode
    && current.outcome.is_none()
}

fn delete_exact_mutex(
  conn: &mut PgConnection,
  operation: &VmOperationDb,
) -> TransactionResult<()> {
  diesel::delete(
    distributed_mutexes::table
      .find(operation.key)
      .filter(distributed_mutexes::resource_kind.eq("Vm"))
      .filter(distributed_mutexes::resource_key.eq(&operation.vm_key))
      .filter(distributed_mutexes::node_key.eq(&operation.node_name)),
  )
  .execute(conn)?;
  Ok(())
}

fn accept_on_conn<F>(
  conn: &mut PgConnection,
  expected: &Vm,
  replacement: Option<&SpecDb>,
  operation: &VmOperationDb,
  write_pending: &mut F,
) -> TransactionResult<Vm>
where
  F: FnMut(
    &mut PgConnection,
    &str,
    ObjPsStatusUpdate,
  ) -> diesel::QueryResult<ObjPsStatusDb>,
{
  let mut vm = vms::table
    .find(&expected.spec.vm_key)
    .for_update()
    .first::<VmDb>(conn)?;
  let status = object_process_statuses::table
    .find(&vm.status_key)
    .for_update()
    .first::<ObjPsStatusDb>(conn)?;
  if vm.spec_key != expected.spec.key {
    return Err(conflict("VM specification changed during acceptance"));
  }
  let previous_operation = vm_operations::table
    .find(&vm.key)
    .for_update()
    .first::<VmOperationDb>(conn)
    .optional()?;
  if previous_operation.is_some_and(|operation| operation.outcome.is_none())
    || pending_phase(&status.actual)
  {
    return Err(conflict("VM has an unresolved pending operation"));
  }
  let accepted_spec_key = replacement.map_or(vm.spec_key, |spec| spec.key);
  if operation.vm_key != vm.key
    || operation.spec_key != accepted_spec_key
    || !pending_phase(&operation.phase)
    || operation.outcome.is_some()
    || operation.observation.is_some()
  {
    return Err(conflict(
      "VM operation does not identify the accepted revision",
    ));
  }
  // A TTL expiry cannot replace a pending durable operation. The admission
  // still has to own its exact mutex when committing a new operation.
  let mutex = distributed_mutexes::table
    .find(operation.key)
    .for_update()
    .first::<DistributedMutexDb>(conn)?;
  if mutex.resource_kind != "Vm"
    || mutex.resource_key != vm.key
    || mutex.node_key != operation.node_name
  {
    return Err(conflict("VM operation does not own its distributed mutex"));
  }
  let spec = match replacement {
    Some(spec) => {
      if spec.kind_name != "Vm" || spec.kind_key != vm.key {
        return Err(conflict(
          "Replacement specification belongs to another VM",
        ));
      }
      let revision = spec.try_to_vm_spec()?;
      diesel::insert_into(specs::table)
        .values(spec)
        .execute(conn)?;
      vm = diesel::update(vms::table.find(&vm.key))
        .set((vms::spec_key.eq(spec.key), vms::name.eq(revision.name)))
        .get_result(conn)?;
      spec.clone()
    }
    None => specs::table.find(vm.spec_key).first::<SpecDb>(conn)?,
  };
  // Replacement of a completed row and the pending phase are covered by the
  // same rollback boundary as the immutable specification and current pointer.
  diesel::delete(vm_operations::table.find(&vm.key)).execute(conn)?;
  diesel::insert_into(vm_operations::table)
    .values(operation)
    .execute(conn)?;
  let wanted = if operation.phase == "destroying" {
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
      actual: Some(operation.phase.clone()),
      prev_actual: Some(status.actual),
      health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
    },
  )?;
  // Conversion is fallible, so do it before committing as well.
  Ok(VmDb::transform((vm, spec, status))?)
}

impl VmDb {
  /// Apply a Docker observation only if no accepted operation or status change
  /// intervened since inspection. The operation UUID also detects a successor
  /// that completed with the same specification and the same status values.
  pub(crate) async fn observe_runtime_status(
    expected: &Vm,
    expected_operation: Option<uuid::Uuid>,
    actual: ObjPsStatusKind,
    pool: &Pool,
  ) -> IoResult<bool> {
    let expected = expected.clone();
    let actual = actual.to_string();
    run_query(pool, move |conn| {
      transaction(conn, |conn| {
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
        let operation = vm_operations::table
          .find(&vm.key)
          .for_update()
          .first::<VmOperationDb>(conn)
          .optional()?;
        if vm.spec_key != expected.spec.key
          || operation.as_ref().map(|operation| operation.key)
            != expected_operation
          || operation
            .as_ref()
            .is_some_and(|operation| operation.outcome.is_none())
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

  /// Commit the desired revision, pending phase and durable owner together.
  /// The caller keeps its lifecycle and operation-file locks through dispatch.
  pub(crate) async fn accept_operation(
    expected: &Vm,
    replacement: Option<SpecDb>,
    operation: VmOperationDb,
    pool: &Pool,
  ) -> IoResult<Vm> {
    Self::accept_operation_with_pending_write(
      expected,
      replacement,
      operation,
      pool,
      |conn, key, update| {
        diesel::update(object_process_statuses::table.find(key))
          .set(update)
          .get_result(conn)
      },
    )
    .await
  }

  pub(crate) async fn accept_operation_with_pending_write<F>(
    expected: &Vm,
    replacement: Option<SpecDb>,
    operation: VmOperationDb,
    pool: &Pool,
    mut write_pending: F,
  ) -> IoResult<Vm>
  where
    F: FnMut(
        &mut PgConnection,
        &str,
        ObjPsStatusUpdate,
      ) -> diesel::QueryResult<ObjPsStatusDb>
      + Send
      + 'static,
  {
    let expected = expected.clone();
    run_query(pool, move |conn| {
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
}

impl VmOperationDb {
  pub(crate) async fn get(vm_key: &str, pool: &Pool) -> IoResult<Option<Self>> {
    let vm_key = vm_key.to_owned();
    run_query(pool, move |conn| {
      vm_operations::table
        .find(vm_key)
        .first::<Self>(conn)
        .optional()
        .map_err(|error| {
          IoError::from(VmOperationTransactionError::from(error))
        })
    })
    .await
  }

  /// Keyset pagination lets unknown owners remain pending without starving
  /// later candidates during bounded recovery passes.
  pub(crate) async fn pending_for_node(
    node: &str,
    after: Option<&str>,
    limit: i64,
    pool: &Pool,
  ) -> IoResult<Vec<Self>> {
    let node = node.to_owned();
    let after = after.map(str::to_owned);
    run_query(pool, move |conn| {
      let mut query = vm_operations::table
        .filter(vm_operations::node_name.eq(node))
        .filter(vm_operations::outcome.is_null())
        .into_boxed();
      if let Some(after) = after {
        query = query.filter(vm_operations::vm_key.gt(after));
      }
      query
        .order(vm_operations::vm_key.asc())
        .limit(limit.clamp(1, 128))
        .load::<Self>(conn)
        .map_err(|error| {
          IoError::from(VmOperationTransactionError::from(error))
        })
    })
    .await
  }

  /// Recover only the exact abandoned pending revision, retaining its runtime
  /// evidence while atomically releasing only that operation's mutex.
  pub(crate) async fn finish(
    &self,
    actual: ObjPsStatusKind,
    outcome: &str,
    observation: serde_json::Value,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = self.clone();
    let actual = actual.to_string();
    let outcome = outcome.to_owned();
    run_query(pool, move |conn| {
      transaction(conn, |conn| {
        if pending_phase(&actual) {
          return Err(conflict(
            "Recovery outcome must leave the pending phase",
          ));
        }
        let Some(vm) = vms::table
          .find(&operation.vm_key)
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
        let current = vm_operations::table
          .find(&vm.key)
          .for_update()
          .first::<Self>(conn)
          .optional()?;
        if vm.spec_key != operation.spec_key
          || !current
            .as_ref()
            .is_some_and(|current| same_operation(current, &operation))
        {
          return Ok(false);
        }
        diesel::update(object_process_statuses::table.find(&vm.status_key))
          .set(ObjPsStatusUpdate {
            wanted: Some(actual.clone()),
            prev_wanted: Some(status.wanted),
            actual: Some(actual.clone()),
            prev_actual: Some(status.actual),
            health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
          })
          .execute(conn)?;
        diesel::update(vm_operations::table.find(&vm.key))
          .set((
            vm_operations::outcome.eq(&outcome),
            vm_operations::observation.eq(&observation),
          ))
          .execute(conn)?;
        delete_exact_mutex(conn, &operation)?;
        Ok(true)
      })
    })
    .await
  }

  /// Executor completion keeps the runtime's terminal status. A failure that
  /// still has a pending phase is left recoverable after the live owner exits.
  pub(crate) async fn complete(
    &self,
    outcome: &str,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = self.clone();
    let outcome = outcome.to_owned();
    run_query(pool, move |conn| {
      transaction(conn, |conn| {
        let Some(vm) = vms::table
          .find(&operation.vm_key)
          .for_update()
          .first::<VmDb>(conn)
          .optional()?
        else {
          // Successful deletion cascades the operation row, not its mutex.
          delete_exact_mutex(conn, &operation)?;
          return Ok(true);
        };
        let status = object_process_statuses::table
          .find(&vm.status_key)
          .for_update()
          .first::<ObjPsStatusDb>(conn)?;
        let current = vm_operations::table
          .find(&vm.key)
          .for_update()
          .first::<Self>(conn)
          .optional()?;
        if vm.spec_key != operation.spec_key
          || pending_phase(&status.actual)
          || !current
            .as_ref()
            .is_some_and(|current| same_operation(current, &operation))
        {
          return Ok(false);
        }
        diesel::update(vm_operations::table.find(&vm.key))
          .set(vm_operations::outcome.eq(&outcome))
          .execute(conn)?;
        delete_exact_mutex(conn, &operation)?;
        Ok(true)
      })
    })
    .await
  }
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::{system::ObjPsStatusPartial, vm_spec::VmSpecPartial};

  use crate::{
    models::{
      DistributedMutexPartial, NamespaceDb, SystemState, VmOperationOwner,
    },
    repositories::generic::{
      RepositoryCreate, RepositoryDelByPk, RepositoryReadBy, RepositoryUpdate,
    },
    utils::tests::{TestSystem, gen_test_system},
    vars,
  };

  use super::*;

  fn no_routes(_: &mut ntex::web::ServiceConfig<SystemState>) {}

  struct Fixture {
    system: TestSystem,
    namespace: String,
    vm: Vm,
    mutexes: Vec<uuid::Uuid>,
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
          name: "guest".to_owned(),
          image: "/transaction-test-original.qcow2".to_owned(),
          ..Default::default()
        },
      )
      .unwrap();
      let spec = SpecDb::create_from(spec, pool).await.unwrap();
      let status = ObjPsStatusDb::create_from(
        ObjPsStatusPartial {
          key: key.clone(),
          wanted: ObjPsStatusKind::Stop,
          prev_wanted: ObjPsStatusKind::Create,
          actual: ObjPsStatusKind::Stop,
          prev_actual: ObjPsStatusKind::Create,
          health: ObjPsHealthStatusKind::Unknown,
        },
        pool,
      )
      .await
      .unwrap();
      let vm = VmDb::create_from(
        VmDb {
          key: key.clone(),
          name: "guest".to_owned(),
          created_at: chrono::Utc::now().naive_utc(),
          namespace_name: namespace.clone(),
          status_key: key,
          spec_key: spec.key,
        },
        pool,
      )
      .await
      .unwrap();
      Self {
        system,
        namespace,
        vm: VmDb::transform((vm, spec, status)).unwrap(),
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
          name: "guest".to_owned(),
          image: "/transaction-test-replacement.qcow2".to_owned(),
          ..Default::default()
        },
      )
      .unwrap()
    }

    async fn operation(
      &mut self,
      revision: uuid::Uuid,
      phase: ObjPsStatusKind,
    ) -> VmOperationDb {
      let mutex = DistributedMutexDb::create_from(
        DistributedMutexPartial {
          action: phase.to_string(),
          resource_kind: "Vm".to_owned(),
          resource_key: self.vm.spec.vm_key.clone(),
          node_key: self.system.state.inner.config.hostname.clone(),
          acquired_by: "vm-transaction-test".to_owned(),
        },
        self.pool(),
      )
      .await
      .unwrap();
      self.mutexes.push(mutex.key);
      VmOperationDb::new(
        &VmOperationOwner {
          key: mutex.key,
          node_name: mutex.node_key,
          owner_boot_id: "transaction-test-boot".to_owned(),
          lock_device: 1,
          lock_inode: 2,
        },
        &self.vm.spec.vm_key,
        revision,
        phase,
      )
    }

    async fn cleanup(self) {
      for mutex in &self.mutexes {
        DistributedMutexDb::del_by_pk(mutex, self.pool())
          .await
          .unwrap();
      }
      VmDb::clear_by_pk(&self.vm.spec.vm_key, self.pool())
        .await
        .unwrap();
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
    let operation =
      fixture.operation(revision, ObjPsStatusKind::Updating).await;
    let operation_key = operation.key;
    let error = VmDb::accept_operation_with_pending_write(
      &fixture.vm,
      Some(replacement),
      operation,
      fixture.pool(),
      move |conn, key, update| {
        // Prove the attempted mutations are visible on this transaction's
        // connection before making the pending-status statement fail.
        assert_eq!(
          vms::table
            .find(key)
            .select(vms::spec_key)
            .first::<uuid::Uuid>(conn)?,
          revision
        );
        assert_eq!(
          vm_operations::table
            .find(key)
            .select(vm_operations::key)
            .first::<uuid::Uuid>(conn)?,
          operation_key
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
    let specs =
      SpecDb::read_by_kind_key(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].key, fixture.vm.spec.key);
    assert!(
      VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .is_none()
    );
    // Rejected preparation leaves cleanup to its admission owner, preserving
    // the mutex until that owner's exact-UUID cleanup runs.
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
    let operation =
      fixture.operation(revision, ObjPsStatusKind::Updating).await;
    let accepted = VmDb::accept_operation(
      &fixture.vm,
      Some(replacement),
      operation.clone(),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(accepted.spec.key, revision);
    assert_eq!(accepted.spec.image, "/transaction-test-replacement.qcow2");
    assert_eq!(accepted.status.actual, ObjPsStatusKind::Updating);
    assert_eq!(accepted.status.prev_actual, ObjPsStatusKind::Stop);
    assert_eq!(accepted.status.wanted, ObjPsStatusKind::Start);
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.spec.key, accepted.spec.key);
    assert_eq!(current.status, accepted.status);
    assert_eq!(
      VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap(),
      Some(operation)
    );
    assert_eq!(
      SpecDb::read_by_kind_key(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .len(),
      2
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_start_acceptance_commits_legacy_mac_in_accepted_revision() {
    let mut fixture = Fixture::new().await;
    assert!(fixture.vm.spec.mac_address.is_none());
    let admission = fixture
      .operation(fixture.vm.spec.key, ObjPsStatusKind::Starting)
      .await;
    let owner = VmOperationOwner {
      key: admission.key,
      node_name: admission.node_name,
      owner_boot_id: admission.owner_boot_id,
      lock_device: admission.lock_device,
      lock_inode: admission.lock_inode,
    };
    let accepted = VmDb::prepare_start(
      &fixture.vm.spec.vm_key,
      owner,
      &fixture.system.state,
    )
    .await
    .unwrap();
    assert_ne!(accepted.spec.key, fixture.vm.spec.key);
    assert!(accepted.spec.mac_address.is_some());
    assert_eq!(accepted.status.actual, ObjPsStatusKind::Starting);
    let operation = VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(operation.key, admission.key);
    assert_eq!(operation.spec_key, accepted.spec.key);
    // The executor's legacy backfill path must retain the accepted generation.
    let runtime_spec =
      VmDb::ensure_mac_address(&fixture.vm.spec.vm_key, &fixture.system.state)
        .await
        .unwrap();
    assert_eq!(runtime_spec.spec.key, accepted.spec.key);
    assert_eq!(runtime_spec.spec.mac_address, accepted.spec.mac_address);
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_recovery_and_completion_preserve_same_revision_successor() {
    let mut fixture = Fixture::new().await;
    let operation = fixture
      .operation(fixture.vm.spec.key, ObjPsStatusKind::Starting)
      .await;
    VmDb::accept_operation(
      &fixture.vm,
      None,
      operation.clone(),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      operation
        .finish(
          ObjPsStatusKind::Stop,
          "interrupted",
          serde_json::json!({"runtime": "absent"}),
          fixture.pool()
        )
        .await
        .unwrap()
    );
    let recovered =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    let successor = fixture
      .operation(recovered.spec.key, ObjPsStatusKind::Starting)
      .await;
    VmDb::accept_operation(&recovered, None, successor.clone(), fixture.pool())
      .await
      .unwrap();
    assert!(
      !operation
        .finish(
          ObjPsStatusKind::Start,
          "interrupted",
          serde_json::json!({"stale": true}),
          fixture.pool()
        )
        .await
        .unwrap()
    );
    assert!(
      !operation
        .complete("completed", fixture.pool())
        .await
        .unwrap()
    );
    assert_eq!(
      VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap(),
      Some(successor.clone())
    );
    assert!(
      DistributedMutexDb::read_by_pk(&successor.key, fixture.pool())
        .await
        .is_ok()
    );
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.status.actual, ObjPsStatusKind::Starting);
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_pending_owner_blocks_acceptance_after_mutex_collection() {
    let mut fixture = Fixture::new().await;
    let original = fixture
      .operation(fixture.vm.spec.key, ObjPsStatusKind::Starting)
      .await;
    let accepted = VmDb::accept_operation(
      &fixture.vm,
      None,
      original.clone(),
      fixture.pool(),
    )
    .await
    .unwrap();
    DistributedMutexDb::del_by_pk(&original.key, fixture.pool())
      .await
      .unwrap();
    let successor = fixture
      .operation(fixture.vm.spec.key, ObjPsStatusKind::Starting)
      .await;
    let error = VmDb::accept_operation(
      &accepted,
      None,
      successor.clone(),
      fixture.pool(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("unresolved pending operation"));
    assert_eq!(
      VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap(),
      Some(original)
    );
    assert!(
      DistributedMutexDb::read_by_pk(&successor.key, fixture.pool())
        .await
        .is_ok()
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_observation_preserves_pending_and_completed_same_revision_successors()
   {
    let mut fixture = Fixture::new().await;
    let first = fixture
      .operation(fixture.vm.spec.key, ObjPsStatusKind::Starting)
      .await;
    VmDb::accept_operation(&fixture.vm, None, first.clone(), fixture.pool())
      .await
      .unwrap();
    assert!(
      first
        .finish(
          ObjPsStatusKind::Stop,
          "interrupted",
          serde_json::Value::Null,
          fixture.pool()
        )
        .await
        .unwrap()
    );
    let snapshot =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    let successor = fixture
      .operation(snapshot.spec.key, ObjPsStatusKind::Starting)
      .await;
    VmDb::accept_operation(&snapshot, None, successor.clone(), fixture.pool())
      .await
      .unwrap();
    assert!(
      !VmDb::observe_runtime_status(
        &snapshot,
        Some(first.key),
        ObjPsStatusKind::Fail,
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      successor
        .finish(
          ObjPsStatusKind::Stop,
          "interrupted",
          serde_json::Value::Null,
          fixture.pool()
        )
        .await
        .unwrap()
    );
    let current =
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(current.spec.key, snapshot.spec.key);
    assert_eq!(
      current.status, snapshot.status,
      "the completed successor restored identical status values"
    );
    assert!(
      !VmDb::observe_runtime_status(
        &snapshot,
        Some(first.key),
        ObjPsStatusKind::Fail,
        fixture.pool()
      )
      .await
      .unwrap()
    );
    // A fresh observation associated with the latest completed operation works.
    assert!(
      VmDb::observe_runtime_status(
        &current,
        Some(successor.key),
        ObjPsStatusKind::Start,
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert_eq!(
      VmDb::transform_read_by_pk(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .status
        .actual,
      ObjPsStatusKind::Start
    );
    assert_eq!(
      VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap()
        .unwrap()
        .key,
      successor.key
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn vm_recovery_settles_crash_after_runtime_status_before_completion() {
    let mut fixture = Fixture::new().await;
    let operation = fixture
      .operation(fixture.vm.spec.key, ObjPsStatusKind::Starting)
      .await;
    VmDb::accept_operation(
      &fixture.vm,
      None,
      operation.clone(),
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
    let observation = serde_json::json!({"container_id": "owned-container", "spec_key": operation.spec_key, "running": true});
    let resize_error = utils::container::vm::runtime_resources(
      &fixture.vm.spec.vm_key,
      &fixture.system.state,
    )
    .await
    .unwrap_err();
    assert!(
      resize_error
        .to_string()
        .contains("awaiting completion or recovery")
    );
    assert_eq!(
      VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
        .await
        .unwrap(),
      Some(operation.clone())
    );
    assert!(
      operation
        .finish(
          ObjPsStatusKind::Start,
          "interrupted",
          observation.clone(),
          fixture.pool()
        )
        .await
        .unwrap()
    );
    assert!(
      !operation
        .finish(
          ObjPsStatusKind::Stop,
          "interrupted",
          serde_json::Value::Null,
          fixture.pool()
        )
        .await
        .unwrap()
    );
    let settled = VmOperationDb::get(&fixture.vm.spec.vm_key, fixture.pool())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
    assert_eq!(settled.observation, Some(observation));
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
}
