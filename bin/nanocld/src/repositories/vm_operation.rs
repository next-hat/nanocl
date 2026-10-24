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
