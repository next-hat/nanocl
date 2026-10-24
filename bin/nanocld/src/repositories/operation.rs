use diesel::prelude::*;
use nanocl_error::io::{IoError, IoResult};

use crate::{
  models::{
    OperationDb, OperationOwner, OperationTransactionError, Pool,
    ResourceIncarnationDb,
  },
  schema::{distributed_mutexes, operations, resource_incarnations, specs},
  utils,
};

pub(crate) type TransactionResult<T> = Result<T, OperationTransactionError>;

/// The worker, rather than its cancellable awaiting future, owns the keepalive
/// through connection acquisition, every statement, rollback and COMMIT.
pub(crate) async fn run_query_owned<T, F, G>(
  pool: &Pool,
  keepalive: G,
  query: F,
) -> IoResult<T>
where
  T: Send + 'static,
  F: FnOnce(&mut PgConnection) -> IoResult<T> + Send + 'static,
  G: Send + 'static,
{
  let pool = pool.clone();
  ntex::rt::spawn_blocking(move || {
    let _keepalive = keepalive;
    let mut conn = utils::store::get_pool_conn(&pool)?;
    query(&mut conn)
  })
  .await
  .map_err(|error| {
    IoError::interrupted("Operation database query", &error.to_string())
  })?
}

/// Runtime bookkeeping can arrive after its caller disappears. Fence those
/// SQL writes with the accepted owner as well as retaining its local guard.
pub(crate) async fn run_mutation_owned<T, F, G>(
  pool: &Pool,
  keepalive: G,
  owner: Option<OperationOwner>,
  mut query: F,
) -> IoResult<T>
where
  T: Send + 'static,
  F: FnMut(&mut PgConnection) -> TransactionResult<T> + Send + 'static,
  G: Send + 'static,
{
  run_query_owned(pool, keepalive, move |conn| {
    transaction(conn, |conn| {
      if let Some(owner) = &owner {
        let identity = lock_incarnation(conn, owner.incarnation_key)?;
        if identity.deleted_at.is_some()
          || identity.resource_kind != owner.resource_kind
          || identity.resource_key != owner.resource_key
          || identity.current_operation_key != Some(owner.key)
        {
          return Err(conflict(
            "Runtime mutation no longer owns this incarnation",
          ));
        }
        let current = operations::table
          .find(owner.key)
          .for_update()
          .first::<OperationDb>(conn)
          .optional()?;
        if !current.is_some_and(|operation| {
          operation.owner() == *owner && operation.outcome.is_none()
        }) {
          return Err(conflict(
            "Runtime mutation no longer owns the pending operation",
          ));
        }
      }
      query(conn)
    })
  })
  .await
}

/// Only SQL is retried. Callers fix accepted identities and inputs beforehand.
pub(crate) fn transaction<T>(
  conn: &mut PgConnection,
  mut query: impl FnMut(&mut PgConnection) -> TransactionResult<T>,
) -> IoResult<T> {
  for attempt in 0..3 {
    let result = conn
      .build_transaction()
      .serializable()
      .run::<_, OperationTransactionError, _>(&mut query);
    match result {
      Err(OperationTransactionError::Database(
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

pub(crate) fn conflict(message: &str) -> OperationTransactionError {
  IoError::with_context(
    "Operation persistence",
    std::io::Error::new(std::io::ErrorKind::WouldBlock, message),
  )
  .into()
}

pub(crate) fn lock_incarnation(
  conn: &mut PgConnection,
  key: uuid::Uuid,
) -> TransactionResult<ResourceIncarnationDb> {
  Ok(
    resource_incarnations::table
      .find(key)
      .for_update()
      .first(conn)?,
  )
}

pub(crate) fn same_identity(
  current: &OperationDb,
  expected: &OperationDb,
) -> bool {
  current.key == expected.key
    && current.incarnation_key == expected.incarnation_key
    && current.resource_kind == expected.resource_kind
    && current.resource_key == expected.resource_key
    && current.action == expected.action
    && current.spec_key == expected.spec_key
    && current.node_name == expected.node_name
    && current.owner_boot_id == expected.owner_boot_id
    && current.lock_device == expected.lock_device
    && current.lock_inode == expected.lock_inode
    && current.accepted_at == expected.accepted_at
}

pub(crate) fn lock_current(
  conn: &mut PgConnection,
  expected: &OperationDb,
) -> TransactionResult<Option<ResourceIncarnationDb>> {
  let Some(identity) = resource_incarnations::table
    .find(expected.incarnation_key)
    .for_update()
    .first::<ResourceIncarnationDb>(conn)
    .optional()?
  else {
    return Ok(None);
  };
  if identity.resource_kind != expected.resource_kind
    || identity.resource_key != expected.resource_key
    || identity.current_operation_key != Some(expected.key)
  {
    return Ok(None);
  }
  let current = operations::table
    .find(expected.key)
    .for_update()
    .first::<OperationDb>(conn)
    .optional()?;
  if !current.as_ref().is_some_and(|current| {
    same_identity(current, expected) && current.outcome.is_none()
  }) {
    return Ok(None);
  }
  Ok(Some(identity))
}

/// Caller locks the retained identity first. This is also the owner-file cleanup
/// fence, so absence after that lock cannot race an earlier uncertain COMMIT.
pub(crate) fn accept_on_conn(
  conn: &mut PgConnection,
  identity: &ResourceIncarnationDb,
  operation: &OperationDb,
) -> TransactionResult<()> {
  if identity.deleted_at.is_some()
    || identity.key != operation.incarnation_key
    || identity.resource_kind != operation.resource_kind
    || identity.resource_key != operation.resource_key
    || operation.outcome.is_some()
    || operation.completed_at.is_some()
    || operation.diagnostic.is_some()
  {
    return Err(conflict(
      "Operation does not identify this live incarnation",
    ));
  }
  if let Some(key) = identity.current_operation_key {
    let previous = operations::table
      .find(key)
      .for_update()
      .first::<OperationDb>(conn)?;
    if previous.outcome.is_none() {
      return Err(conflict("Resource has an unresolved pending operation"));
    }
  }
  let spec = specs::table
    .find(operation.spec_key)
    .first::<crate::models::SpecDb>(conn)?;
  if spec.incarnation_key != Some(identity.key)
    || spec.kind_name != operation.resource_kind
    || spec.kind_key != operation.resource_key
  {
    return Err(conflict(
      "Accepted specification belongs to another resource incarnation",
    ));
  }
  diesel::insert_into(operations::table)
    .values(operation)
    .execute(conn)?;
  diesel::update(resource_incarnations::table.find(identity.key))
    .set(resource_incarnations::current_operation_key.eq(operation.key))
    .execute(conn)?;
  Ok(())
}

pub(crate) fn complete_on_conn(
  conn: &mut PgConnection,
  operation: &OperationDb,
  outcome: &str,
  diagnostic: Option<&serde_json::Value>,
) -> TransactionResult<()> {
  if lock_current(conn, operation)?.is_none() {
    return Err(conflict("Operation is no longer current and pending"));
  }
  if !matches!(outcome, "completed" | "failed" | "interrupted") {
    return Err(conflict("Operation outcome is not terminal"));
  }
  diesel::update(operations::table.find(operation.key))
    .set((
      operations::outcome.eq(outcome),
      operations::completed_at.eq(chrono::Utc::now().naive_utc()),
      operations::diagnostic.eq(diagnostic),
    ))
    .execute(conn)?;
  delete_exact_mutex(conn, operation)
}

pub(crate) fn delete_exact_mutex(
  conn: &mut PgConnection,
  operation: &OperationDb,
) -> TransactionResult<()> {
  diesel::delete(
    distributed_mutexes::table
      .find(operation.key)
      .filter(distributed_mutexes::resource_kind.eq(&operation.resource_kind))
      .filter(distributed_mutexes::resource_key.eq(&operation.resource_key))
      .filter(distributed_mutexes::node_key.eq(&operation.node_name)),
  )
  .execute(conn)?;
  Ok(())
}

#[cfg(test)]
impl ResourceIncarnationDb {
  pub(crate) async fn get(
    key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<Option<Self>> {
    run_query_owned(pool, (), move |conn| {
      Ok(
        resource_incarnations::table
          .find(key)
          .first(conn)
          .optional()
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }
}

impl OperationDb {
  /// Retained internal lookup; exposing operation history through an API is separate.
  #[cfg_attr(not(test), allow(dead_code))]
  pub(crate) async fn get(
    key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<Option<Self>> {
    run_query_owned(pool, (), move |conn| {
      Ok(
        operations::table
          .find(key)
          .first(conn)
          .optional()
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  pub(crate) async fn current(
    kind: &str,
    key: &str,
    pool: &Pool,
  ) -> IoResult<Option<Self>> {
    let (kind, key) = (kind.to_owned(), key.to_owned());
    run_query_owned(pool, (), move |conn| {
      Ok(
        operations::table
          .inner_join(
            resource_incarnations::table.on(
              resource_incarnations::current_operation_key
                .eq(operations::key.nullable()),
            ),
          )
          .filter(resource_incarnations::resource_kind.eq(kind))
          .filter(resource_incarnations::resource_key.eq(key))
          .filter(resource_incarnations::deleted_at.is_null())
          .select(operations::all_columns)
          .first(conn)
          .optional()
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  #[cfg(test)]
  pub(crate) async fn current_for_incarnation(
    key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<Option<Self>> {
    run_query_owned(pool, (), move |conn| {
      Ok(
        operations::table
          .inner_join(
            resource_incarnations::table.on(
              resource_incarnations::current_operation_key
                .eq(operations::key.nullable()),
            ),
          )
          .filter(resource_incarnations::key.eq(key))
          .select(operations::all_columns)
          .first(conn)
          .optional()
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  /// Bounded internal history query, including deleted resource incarnations.
  /// The VM consumer does not expose a public operation-history API.
  #[cfg_attr(not(test), allow(dead_code))]
  pub(crate) async fn history(
    kind: &str,
    incarnation: uuid::Uuid,
    after: Option<(chrono::NaiveDateTime, uuid::Uuid)>,
    limit: i64,
    pool: &Pool,
  ) -> IoResult<Vec<Self>> {
    let kind = kind.to_owned();
    run_query_owned(pool, (), move |conn| {
      let mut query = operations::table
        .filter(operations::resource_kind.eq(kind))
        .filter(operations::incarnation_key.eq(incarnation))
        .into_boxed();
      if let Some((time, key)) = after {
        query = query.filter(
          operations::accepted_at.gt(time).or(
            operations::accepted_at
              .eq(time)
              .and(operations::key.gt(key)),
          ),
        );
      }
      Ok(
        query
          .order((operations::accepted_at.asc(), operations::key.asc()))
          .limit(limit.clamp(1, 128))
          .load(conn)
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  pub(crate) async fn pending_for_node(
    kind: &str,
    node: &str,
    after: Option<uuid::Uuid>,
    limit: i64,
    pool: &Pool,
  ) -> IoResult<Vec<Self>> {
    let (kind, node) = (kind.to_owned(), node.to_owned());
    run_query_owned(pool, (), move |conn| {
      let mut query = operations::table
        .filter(operations::resource_kind.eq(kind))
        .filter(operations::node_name.eq(node))
        .filter(operations::outcome.is_null())
        .into_boxed();
      if let Some(after) = after {
        query = query.filter(operations::key.gt(after));
      }
      Ok(
        query
          .order(operations::key.asc())
          .limit(limit.clamp(1, 128))
          .load(conn)
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  pub(crate) async fn owner_file_cleanup_eligible<G: Send + 'static>(
    owner: OperationOwner,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(identity) = resource_incarnations::table
          .find(owner.incarnation_key)
          .for_update()
          .first::<ResourceIncarnationDb>(conn)
          .optional()?
        else {
          return Ok(false);
        };
        if identity.resource_kind != owner.resource_kind
          || identity.resource_key != owner.resource_key
        {
          return Ok(false);
        }
        let operation = operations::table
          .find(owner.key)
          .for_update()
          .first::<Self>(conn)
          .optional()?;
        Ok(match operation {
          None => identity.current_operation_key != Some(owner.key),
          Some(operation) => {
            operation.owner() == owner && operation.outcome.is_some()
          }
        })
      })
    })
    .await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    models::{SpecDb, SystemState},
    utils::tests::{TestSystem, gen_test_system},
    vars,
  };

  fn no_routes(_: &mut ntex::web::ServiceConfig<SystemState>) {}

  async fn seed(
    kind: &str,
    key: &str,
    pool: &Pool,
  ) -> (ResourceIncarnationDb, SpecDb) {
    let identity = ResourceIncarnationDb {
      key: uuid::Uuid::new_v4(),
      resource_kind: kind.into(),
      resource_key: key.into(),
      created_at: chrono::Utc::now().naive_utc(),
      deleted_at: None,
      current_operation_key: None,
    };
    let spec = SpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: kind.into(),
      kind_key: key.into(),
      version: vars::VERSION.into(),
      data: serde_json::json!({"test":"accepted input"}),
      metadata: None,
      incarnation_key: Some(identity.key),
    };
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        let identity = diesel::insert_into(resource_incarnations::table)
          .values(&identity)
          .get_result(conn)?;
        let spec = diesel::insert_into(specs::table)
          .values(&spec)
          .get_result(conn)?;
        Ok((identity, spec))
      })
    })
    .await
    .unwrap()
  }

  fn candidate(
    identity: &ResourceIncarnationDb,
    spec: &SpecDb,
    node: &str,
  ) -> OperationDb {
    OperationDb::new(
      &OperationOwner {
        key: uuid::Uuid::new_v4(),
        resource_kind: identity.resource_kind.clone(),
        resource_key: identity.resource_key.clone(),
        incarnation_key: identity.key,
        node_name: node.into(),
        owner_boot_id: "test-boot".into(),
        lock_device: 1,
        lock_inode: 2,
      },
      &identity.resource_kind,
      &identity.resource_key,
      spec.key,
      "start",
    )
  }

  async fn accept(operation: &OperationDb, pool: &Pool) {
    let operation = operation.clone();
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        let identity = lock_incarnation(conn, operation.incarnation_key)?;
        accept_on_conn(conn, &identity, &operation)
      })
    })
    .await
    .unwrap();
  }

  async fn complete(operation: &OperationDb, pool: &Pool) {
    let operation = operation.clone();
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        complete_on_conn(conn, &operation, "completed", None)
      })
    })
    .await
    .unwrap();
  }

  async fn remove_seed(identity: uuid::Uuid, pool: &Pool) {
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        diesel::update(resource_incarnations::table.find(identity))
          .set(
            resource_incarnations::current_operation_key.eq(None::<uuid::Uuid>),
          )
          .execute(conn)?;
        diesel::delete(
          operations::table.filter(operations::incarnation_key.eq(identity)),
        )
        .execute(conn)?;
        diesel::delete(
          specs::table.filter(specs::incarnation_key.eq(identity)),
        )
        .execute(conn)?;
        diesel::delete(resource_incarnations::table.find(identity))
          .execute(conn)?;
        Ok(())
      })
    })
    .await
    .unwrap();
  }

  async fn system() -> TestSystem {
    gen_test_system(no_routes, vars::VERSION).await
  }

  #[ntex::test]
  async fn shared_history_and_pending_queries_isolate_kind_and_incarnation() {
    let system = system().await;
    let pool = &system.state.inner.pool;
    let key = format!("shared-history-{}", uuid::Uuid::new_v4());
    let node = format!("history-node-{}", uuid::Uuid::new_v4());
    let (vm, vm_spec) = seed("Vm", &key, pool).await;
    let (cargo, cargo_spec) = seed("Cargo", &key, pool).await;
    let first = candidate(&vm, &vm_spec, &node);
    let foreign = candidate(&cargo, &cargo_spec, &node);
    accept(&first, pool).await;
    accept(&foreign, pool).await;
    run_mutation_owned(pool, (), Some(first.owner()), |_| Ok(()))
      .await
      .unwrap();
    assert_eq!(
      OperationDb::current("Vm", &key, pool).await.unwrap(),
      Some(first.clone())
    );
    assert_eq!(
      OperationDb::current("Cargo", &key, pool).await.unwrap(),
      Some(foreign.clone())
    );
    assert_eq!(
      OperationDb::pending_for_node("Vm", &node, None, 1, pool)
        .await
        .unwrap(),
      vec![first.clone()]
    );
    assert!(
      OperationDb::pending_for_node("Vm", &node, Some(first.key), 1, pool)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      OperationDb::history("Cargo", vm.key, None, 128, pool)
        .await
        .unwrap()
        .is_empty()
    );
    complete(&first, pool).await;
    let rejected: IoResult<()> =
      run_mutation_owned(pool, (), Some(first.owner()), |_| {
        panic!("terminal operation reached its runtime SQL writer")
      })
      .await;
    assert!(rejected.is_err());
    let successor = candidate(&vm, &vm_spec, &node);
    accept(&successor, pool).await;
    let rejected: IoResult<()> =
      run_mutation_owned(pool, (), Some(first.owner()), |_| {
        panic!("superseded operation reached its runtime SQL writer")
      })
      .await;
    assert!(rejected.is_err());
    complete(&successor, pool).await;
    // The shared completion helper itself refuses old or terminal identities.
    let old = first.clone();
    assert!(
      run_query_owned(pool, (), move |conn| transaction(conn, |conn| {
        complete_on_conn(conn, &old, "failed", None)
      }))
      .await
      .is_err()
    );
    assert_eq!(
      OperationDb::current("Vm", &key, pool)
        .await
        .unwrap()
        .unwrap()
        .key,
      successor.key
    );
    let history = OperationDb::history("Vm", vm.key, None, 1, pool)
      .await
      .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].key, first.key);
    let next = OperationDb::history(
      "Vm",
      vm.key,
      Some((history[0].accepted_at, history[0].key)),
      1,
      pool,
    )
    .await
    .unwrap();
    assert_eq!(next[0].key, successor.key);
    assert!(
      OperationDb::pending_for_node("Vm", &node, None, 128, pool)
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(
      OperationDb::pending_for_node("Cargo", &node, None, 128, pool)
        .await
        .unwrap(),
      vec![foreign.clone()]
    );
    complete(&foreign, pool).await;
    // The Cargo-only deletion helper must not erase a same-name VM revision.
    // Use an unadopted Cargo revision, as existing Cargo deletion does today.
    let unadopted = SpecDb {
      key: uuid::Uuid::new_v4(),
      incarnation_key: None,
      ..cargo_spec.clone()
    };
    let unadopted_key = unadopted.key;
    run_query_owned(pool, (), move |conn| {
      diesel::insert_into(specs::table)
        .values(unadopted)
        .execute(conn)
        .map_err(OperationTransactionError::from)?;
      Ok(())
    })
    .await
    .unwrap();
    // Remove the synthetic adopted Cargo history before exercising its legacy cleanup.
    remove_seed(cargo.key, pool).await;
    SpecDb::del_by_kind_key("Cargo", &key, pool).await.unwrap();
    assert!(
      run_query_owned(pool, (), move |conn| Ok(
        specs::table
          .find(unadopted_key)
          .first::<SpecDb>(conn)
          .optional()
          .map_err(OperationTransactionError::from)?
      ))
      .await
      .unwrap()
      .is_none()
    );
    assert_eq!(
      OperationDb::get(first.key, pool)
        .await
        .unwrap()
        .unwrap()
        .spec_key,
      vm_spec.key
    );
    remove_seed(vm.key, pool).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn ownership_cleanup_requires_a_fenced_exact_terminal_or_absent_record()
  {
    let system = system().await;
    let pool = &system.state.inner.pool;
    let (identity, spec) =
      seed("Vm", &format!("cleanup-{}", uuid::Uuid::new_v4()), pool).await;
    let operation = candidate(&identity, &spec, "cleanup-node");
    assert!(
      OperationDb::owner_file_cleanup_eligible(operation.owner(), (), pool)
        .await
        .unwrap()
    );
    let mut unknown = operation.owner();
    unknown.incarnation_key = uuid::Uuid::new_v4();
    assert!(
      !OperationDb::owner_file_cleanup_eligible(unknown, (), pool)
        .await
        .unwrap()
    );
    accept(&operation, pool).await;
    assert!(
      !OperationDb::owner_file_cleanup_eligible(operation.owner(), (), pool)
        .await
        .unwrap()
    );
    complete(&operation, pool).await;
    assert!(
      OperationDb::owner_file_cleanup_eligible(operation.owner(), (), pool)
        .await
        .unwrap()
    );
    let mut mismatch = operation.owner();
    mismatch.lock_inode += 1;
    assert!(
      !OperationDb::owner_file_cleanup_eligible(mismatch, (), pool)
        .await
        .unwrap()
    );
    remove_seed(identity.key, pool).await;
    system.state.wait_event_loop().await;
  }

  #[ntex::test]
  async fn cancelled_database_waiter_keeps_owner_until_commit_or_rollback_finishes()
   {
    let system = system().await;
    let pool = &system.state.inner.pool;
    for commit in [true, false] {
      let (identity, spec) =
        seed("Vm", &format!("worker-{}", uuid::Uuid::new_v4()), pool).await;
      let directory = std::env::temp_dir()
        .join(format!("nanocl-database-owner-{}", uuid::Uuid::new_v4()));
      let (owner, guard) = utils::operation::create_owner(
        uuid::Uuid::new_v4(),
        "Vm",
        &identity.resource_key,
        identity.key,
        "worker-node",
        directory.to_str().unwrap(),
      )
      .unwrap();
      let operation = OperationDb::new(
        &owner,
        "Vm",
        &identity.resource_key,
        spec.key,
        "start",
      );
      let operation_key = operation.key;
      let incarnation = identity.key;
      let worker_pool = pool.clone();
      let (entered, seen) = tokio::sync::oneshot::channel();
      let (release, gate) = std::sync::mpsc::channel();
      let (worker_done, done) = tokio::sync::oneshot::channel::<()>();
      let caller = ntex::rt::spawn(async move {
        run_query_owned(&worker_pool, (guard, worker_done), move |conn| {
          let mut entered = Some(entered);
          transaction(conn, |conn| {
            let identity = lock_incarnation(conn, incarnation)?;
            // Block after the authoritative fence, before acceptance writes.
            if let Some(entered) = entered.take() {
              entered.send(()).unwrap();
              gate.recv().unwrap();
            }
            accept_on_conn(conn, &identity, &operation)?;
            if commit {
              Ok(())
            } else {
              Err(conflict("Injected acceptance rollback"))
            }
          })
        })
        .await
      });
      seen.await.unwrap();
      caller.cancel();
      assert!(
        utils::operation::abandoned_owner(
          &owner,
          "worker-node",
          directory.to_str().unwrap()
        )
        .unwrap()
        .is_none(),
        "cancelled waiter released the worker's flock"
      );
      // A cancelled waiter must not release the installation controller lock
      // while its blocking database worker can still commit or roll back.
      use std::os::fd::AsRawFd;
      let controller_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("run/operations.gate"))
        .unwrap();
      assert_eq!(
        unsafe {
          libc::flock(
            controller_file.as_raw_fd(),
            libc::LOCK_EX | libc::LOCK_NB,
          )
        },
        -1
      );
      assert_eq!(
        std::io::Error::last_os_error().kind(),
        std::io::ErrorKind::WouldBlock
      );
      // A cleanup request enters while acceptance owns the incarnation fence.
      // It must decide from the committed/rolled-back result, never a stale absence.
      let cleaner_pool = pool.clone();
      let cleaner_owner = owner.clone();
      let cleaner = ntex::rt::spawn(async move {
        OperationDb::owner_file_cleanup_eligible(
          cleaner_owner,
          (),
          &cleaner_pool,
        )
        .await
      });
      release.send(()).unwrap();
      assert!(
        done.await.is_err(),
        "worker completion is signalled by keepalive drop"
      );
      let eligible = cleaner.await.unwrap().unwrap();
      assert_eq!(eligible, !commit);
      let recovered = utils::operation::abandoned_owner(
        &owner,
        "worker-node",
        directory.to_str().unwrap(),
      )
      .unwrap()
      .expect("worker finished but retained its flock");
      let stored = OperationDb::get(operation_key, pool).await.unwrap();
      if commit {
        let stored = stored.unwrap();
        assert!(stored.outcome.is_none());
        assert!(
          !utils::operation::cleanup_owner(&recovered, pool)
            .await
            .unwrap()
        );
        complete(&stored, pool).await;
      } else {
        assert!(stored.is_none());
        assert_eq!(
          ResourceIncarnationDb::get(incarnation, pool)
            .await
            .unwrap()
            .unwrap()
            .current_operation_key,
          None
        );
      }
      assert!(
        utils::operation::cleanup_owner(&recovered, pool)
          .await
          .unwrap()
      );
      drop(recovered);
      std::fs::remove_dir_all(directory).unwrap();
      remove_seed(incarnation, pool).await;
    }
    system.state.wait_event_loop().await;
  }
}
