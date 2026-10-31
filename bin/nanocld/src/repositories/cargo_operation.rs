use diesel::prelude::*;
use nanocl_error::io::IoResult;
use nanocl_stubs::{
  cargo::Cargo,
  system::{ObjPsHealthStatusKind, ObjPsStatusKind},
};

use super::{
  generic::RepositoryReadByTransform,
  operation::{
    self, TransactionResult, conflict, run_query_owned, transaction,
  },
};
use crate::{
  models::{
    AcceptedCargoOperation, CargoDb, DistributedMutexDb, ObjPsStatusDb,
    ObjPsStatusUpdate, OperationDb, Pool, SpecDb,
  },
  schema::{
    cargoes, distributed_mutexes, object_process_statuses, operations,
    resource_incarnations, specs,
  },
};

pub(crate) fn pending_phase(phase: &str) -> bool {
  matches!(phase, "starting" | "updating" | "stopping" | "destroying")
}

fn admission_status(
  action: &str,
) -> TransactionResult<Option<(&'static str, &'static str)>> {
  match action {
    "start" | "restart" => Ok(Some(("start", "starting"))),
    "update" => Ok(Some(("start", "updating"))),
    "stop" => Ok(Some(("stop", "stopping"))),
    "delete" => Ok(Some(("destroy", "destroying"))),
    "stop_process" | "restart_process" | "signal" => Ok(None),
    _ => Err(conflict("Unsupported Cargo operation action")),
  }
}

/// Bootstrap may only reconstruct a definition that has never accepted work.
pub(crate) fn lock_bootstrap_cargo(
  conn: &mut PgConnection,
  expected: &Cargo,
  incarnation: uuid::Uuid,
) -> TransactionResult<CargoDb> {
  let identity = operation::lock_incarnation(conn, incarnation)?;
  let cargo = cargoes::table
    .find(&expected.spec.cargo_key)
    .for_update()
    .first::<CargoDb>(conn)?;
  let status = object_process_statuses::table
    .find(&cargo.status_key)
    .for_update()
    .first::<ObjPsStatusDb>(conn)?;
  if identity.deleted_at.is_some()
    || identity.resource_kind != "Cargo"
    || identity.resource_key != cargo.key
    || identity.current_operation_key.is_some()
    || cargo.incarnation_key != incarnation
    || cargo.spec_key != expected.spec.key
    || pending_phase(&status.actual)
  {
    return Err(conflict(
      "Cargo bootstrap no longer owns an untouched definition",
    ));
  }
  if resource_incarnations::table
    .filter(resource_incarnations::resource_kind.eq("Cargo"))
    .filter(resource_incarnations::resource_key.eq(&cargo.key))
    .filter(resource_incarnations::key.ne(incarnation))
    .select(resource_incarnations::key)
    .first::<uuid::Uuid>(conn)
    .optional()?
    .is_some()
  {
    return Err(conflict(
      "Cargo bootstrap cannot adopt runtime from an earlier incarnation",
    ));
  }
  Ok(cargo)
}

/// The retained identity is always locked before its live definition/status.
fn lock_current_cargo(
  conn: &mut PgConnection,
  operation: &OperationDb,
) -> TransactionResult<Option<(CargoDb, ObjPsStatusDb)>> {
  let Some(identity) = operation::lock_current(conn, operation)? else {
    return Ok(None);
  };
  if identity.deleted_at.is_some() || operation.resource_kind != "Cargo" {
    return Ok(None);
  }
  let Some(cargo) = cargoes::table
    .find(&operation.resource_key)
    .for_update()
    .first::<CargoDb>(conn)
    .optional()?
  else {
    return Ok(None);
  };
  if cargo.incarnation_key != operation.incarnation_key
    || cargo.spec_key != operation.spec_key
  {
    return Ok(None);
  }
  let status = object_process_statuses::table
    .find(&cargo.status_key)
    .for_update()
    .first::<ObjPsStatusDb>(conn)?;
  Ok(Some((cargo, status)))
}

fn write_status(
  conn: &mut PgConnection,
  previous: &ObjPsStatusDb,
  mut update: ObjPsStatusUpdate,
) -> TransactionResult<ObjPsStatusDb> {
  if update
    .actual
    .as_ref()
    .is_some_and(|actual| actual != &previous.actual)
    && update.prev_actual.is_none()
  {
    update.prev_actual = Some(previous.actual.clone());
  }
  if update
    .wanted
    .as_ref()
    .is_some_and(|wanted| wanted != &previous.wanted)
    && update.prev_wanted.is_none()
  {
    update.prev_wanted = Some(previous.wanted.clone());
  }
  Ok(
    diesel::update(object_process_statuses::table.find(&previous.key))
      .set((
        update,
        object_process_statuses::updated_at.eq(chrono::Utc::now().naive_utc()),
      ))
      .get_result(conn)?,
  )
}

impl CargoDb {
  pub(crate) async fn accept_operation<G: Send + 'static>(
    expected: &Cargo,
    replacement: Option<SpecDb>,
    operation: OperationDb,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<AcceptedCargoOperation> {
    let expected = expected.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let identity =
          operation::lock_incarnation(conn, operation.incarnation_key)?;
        let mut cargo = cargoes::table
          .find(&expected.spec.cargo_key)
          .for_update()
          .first::<CargoDb>(conn)?;
        let status = object_process_statuses::table
          .find(&cargo.status_key)
          .for_update()
          .first::<ObjPsStatusDb>(conn)?;
        if identity.deleted_at.is_some()
          || identity.resource_kind != "Cargo"
          || identity.resource_key != cargo.key
          || cargo.incarnation_key != identity.key
          || cargo.spec_key != expected.spec.key
          || operation.resource_kind != "Cargo"
          || operation.resource_key != cargo.key
          || operation.spec_key
            != replacement.as_ref().map_or(cargo.spec_key, |spec| spec.key)
        {
          return Err(conflict(
            "Cargo incarnation or specification changed during acceptance",
          ));
        }
        if pending_phase(&status.actual) {
          return Err(conflict("Cargo has an unresolved pending transition"));
        }
        let pending = admission_status(&operation.action)?;
        let mutex = distributed_mutexes::table
          .find(operation.key)
          .for_update()
          .first::<DistributedMutexDb>(conn)?;
        if mutex.resource_kind != "Cargo"
          || mutex.resource_key != cargo.key
          || mutex.node_key != operation.node_name
          || mutex.action != operation.action
        {
          return Err(conflict("Cargo operation does not own its exact mutex"));
        }
        let spec = match &replacement {
          Some(spec) => {
            if operation.action != "update"
              || spec.kind_name != "Cargo"
              || spec.kind_key != cargo.key
              || spec
                .incarnation_key
                .is_some_and(|key| key != cargo.incarnation_key)
              || spec.try_to_cargo_spec()?.name != cargo.name
            {
              return Err(conflict(
                "Replacement specification belongs to another Cargo identity",
              ));
            }
            let mut spec = spec.clone();
            spec.incarnation_key = Some(cargo.incarnation_key);
            let spec = diesel::insert_into(specs::table)
              .values(&spec)
              .get_result::<SpecDb>(conn)?;
            cargo = diesel::update(cargoes::table.find(&cargo.key))
              .set(cargoes::spec_key.eq(spec.key))
              .get_result(conn)?;
            spec
          }
          None => specs::table.find(cargo.spec_key).first::<SpecDb>(conn)?,
        };
        operation::accept_on_conn(conn, &identity, &operation)?;
        let status = if let Some((wanted, actual)) = pending {
          write_status(
            conn,
            &status,
            ObjPsStatusUpdate {
              wanted: Some(wanted.into()),
              prev_wanted: Some(status.wanted.clone()),
              actual: Some(actual.into()),
              prev_actual: Some(status.actual.clone()),
              health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
            },
          )?
        } else {
          status
        };
        Ok(AcceptedCargoOperation {
          cargo: Self::transform((cargo, spec, status))?,
          operation: operation.clone(),
        })
      })
    })
    .await
  }

  pub(crate) async fn validate_operation<G: Send + 'static>(
    operation: &OperationDb,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        Ok(lock_current_cargo(conn, &operation)?.is_some())
      })
    })
    .await
  }

  #[cfg(test)]
  pub(crate) async fn update_status_owned<G: Send + 'static>(
    operation: &OperationDb,
    update: ObjPsStatusUpdate,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some((_, status)) = lock_current_cargo(conn, &operation)? else {
          return Ok(false);
        };
        write_status(conn, &status, update.clone())?;
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
        let Some((_, status)) = lock_current_cargo(conn, &operation)? else {
          return Ok(false);
        };
        if pending_phase(&status.actual) {
          return Ok(false);
        }
        operation::complete_on_conn(
          conn,
          &operation,
          &outcome,
          diagnostic.as_ref(),
        )?;
        #[cfg(test)]
        if completion_fault::take(&operation) {
          return Err(
            nanocl_error::io::IoError::other(
              "Cargo completion fixture",
              "Injected failure after operation completion writes",
            )
            .into(),
          );
        }
        Ok(true)
      })
    })
    .await
  }

  /// Apply the observed terminal status and operation outcome in one commit.
  /// Only explicit update fields change desired state or rollback markers.
  pub(crate) async fn finish_operation<G: Send + 'static>(
    operation: &OperationDb,
    update: ObjPsStatusUpdate,
    outcome: &str,
    diagnostic: Option<serde_json::Value>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    let outcome = outcome.to_owned();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some((_, status)) = lock_current_cargo(conn, &operation)? else {
          return Ok(false);
        };
        if pending_phase(update.actual.as_deref().unwrap_or(&status.actual)) {
          return Err(conflict(
            "Cargo operation completion must leave the pending phase",
          ));
        }
        write_status(conn, &status, update.clone())?;
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

  pub(crate) async fn delete_operation<G: Send + 'static>(
    operation: &OperationDb,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        if operation.action != "delete" {
          return Ok(false);
        }
        let Some((cargo, _)) = lock_current_cargo(conn, &operation)? else {
          return Ok(false);
        };
        diesel::delete(cargoes::table.find(&cargo.key)).execute(conn)?;
        diesel::delete(object_process_statuses::table.find(&cargo.status_key))
          .execute(conn)?;
        diesel::update(
          resource_incarnations::table.find(cargo.incarnation_key),
        )
        .set(
          resource_incarnations::deleted_at.eq(chrono::Utc::now().naive_utc()),
        )
        .execute(conn)?;
        operation::complete_on_conn(conn, &operation, "completed", None)?;
        Ok(true)
      })
    })
    .await
  }

  pub(crate) async fn observe_runtime_status<G: Send + 'static>(
    expected: &Cargo,
    expected_incarnation: uuid::Uuid,
    expected_operation: Option<uuid::Uuid>,
    actual: ObjPsStatusKind,
    health: ObjPsHealthStatusKind,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let expected = expected.clone();
    let actual = actual.to_string();
    let health = health.to_string();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let identity = operation::lock_incarnation(conn, expected_incarnation)?;
        if identity.deleted_at.is_some()
          || identity.resource_kind != "Cargo"
          || identity.resource_key != expected.spec.cargo_key
          || identity.current_operation_key != expected_operation
        {
          return Ok(false);
        }
        let Some(cargo) = cargoes::table
          .find(&expected.spec.cargo_key)
          .for_update()
          .first::<CargoDb>(conn)
          .optional()?
        else {
          return Ok(false);
        };
        let status = object_process_statuses::table
          .find(&cargo.status_key)
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
        if cargo.incarnation_key != expected_incarnation
          || cargo.spec_key != expected.spec.key
          || nanocl_stubs::system::ObjPsStatus::try_from(status.clone())?
            != expected.status
          || pending_phase(&status.actual)
          || pending_phase(&actual)
          || (status.actual == actual && status.health == health)
        {
          return Ok(false);
        }
        write_status(
          conn,
          &status,
          ObjPsStatusUpdate {
            actual: Some(actual.clone()),
            health: Some(health.clone()),
            ..Default::default()
          },
        )?;
        Ok(true)
      })
    })
    .await
  }
}

#[cfg(test)]
pub(crate) mod completion_fault {
  use std::{
    collections::HashMap,
    sync::{
      Arc, LazyLock, Mutex,
      atomic::{AtomicBool, Ordering},
    },
  };

  use crate::models::OperationDb;

  type Faults = HashMap<(String, String), Arc<AtomicBool>>;
  static FAULTS: LazyLock<Mutex<Faults>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

  pub(crate) struct CompletionFault {
    identity: (String, String),
    triggered: Arc<AtomicBool>,
  }

  impl CompletionFault {
    pub(crate) fn was_triggered(&self) -> bool {
      self.triggered.load(Ordering::SeqCst)
    }
  }

  impl Drop for CompletionFault {
    fn drop(&mut self) {
      let mut faults = FAULTS.lock().unwrap();
      if faults
        .get(&self.identity)
        .is_some_and(|fault| Arc::ptr_eq(fault, &self.triggered))
      {
        faults.remove(&self.identity);
      }
    }
  }

  /// Fail only this Cargo/action after its normal completion SQL, forcing
  /// rollback of both the terminal outcome and exact mutex removal.
  pub(crate) fn fail_next_completion(
    key: &str,
    action: &str,
  ) -> CompletionFault {
    let identity = (key.to_owned(), action.to_owned());
    let triggered = Arc::new(AtomicBool::new(false));
    assert!(
      FAULTS
        .lock()
        .unwrap()
        .insert(identity.clone(), triggered.clone())
        .is_none(),
      "completion fault already armed for this Cargo/action"
    );
    CompletionFault {
      identity,
      triggered,
    }
  }

  pub(super) fn take(operation: &OperationDb) -> bool {
    FAULTS
      .lock()
      .unwrap()
      .get(&(operation.resource_key.clone(), operation.action.clone()))
      .is_some_and(|triggered| !triggered.swap(true, Ordering::SeqCst))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    models::{
      CargoReplicaDb, CargoReplicaProcessDb, CargoReplicaProcessRole,
      DistributedMutexPartial, NamespaceDb, NewCargoReplicaDb,
      NewCargoReplicaProcessDb, NodeDb, OperationOwner, ProcessDb,
      ProcessUpdateDb,
    },
    repositories::generic::{RepositoryCreate, RepositoryReadBy},
    utils::tests::{TestSystem, gen_test_system},
    vars,
  };
  use nanocl_stubs::{
    cargo_spec::CargoSpec,
    process::{ProcessKind, ProcessPartial},
    system::ObjPsStatusPartial,
  };

  fn no_routes(_: &mut ntex::web::ServiceConfig<crate::models::SystemState>) {}

  struct Fixture {
    system: TestSystem,
    namespace: String,
    cargo: Cargo,
    nodes: Vec<String>,
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
      let namespace =
        format!("cargo-operation-{}", uuid::Uuid::new_v4().simple());
      let pool = &system.state.inner.pool;
      NamespaceDb::create_from(NamespaceDb::new(&namespace), pool)
        .await
        .unwrap();
      let key = format!("{namespace}.service");
      let spec = SpecDb::try_from_cargo_spec(
        &key,
        vars::VERSION,
        &CargoSpec {
          name: "service".into(),
          ..Default::default()
        },
      )
      .unwrap();
      let cargo = CargoDb::create_with_spec(
        &key,
        "service",
        &namespace,
        spec,
        stopped(&key),
        pool,
      )
      .await
      .unwrap();
      Self {
        system,
        namespace,
        cargo,
        nodes: Vec::new(),
      }
    }

    fn pool(&self) -> &Pool {
      &self.system.state.inner.pool
    }

    fn replacement(&self) -> SpecDb {
      SpecDb::try_from_cargo_spec(
        &self.cargo.spec.cargo_key,
        vars::VERSION,
        &CargoSpec {
          name: "service".into(),
          metadata: Some(serde_json::json!({"revision":"replacement"})),
          ..Default::default()
        },
      )
      .unwrap()
    }

    async fn operation(
      &self,
      spec_key: uuid::Uuid,
      action: &str,
    ) -> OperationDb {
      let mutex = DistributedMutexDb::create_from(
        DistributedMutexPartial {
          action: action.into(),
          resource_kind: "Cargo".into(),
          resource_key: self.cargo.spec.cargo_key.clone(),
          node_key: "cargo-operation-coordinator".into(),
          acquired_by: "cargo-operation-fixture".into(),
        },
        self.pool(),
      )
      .await
      .unwrap();
      let owner = OperationOwner {
        key: mutex.key,
        incarnation_key: CargoDb::incarnation(
          &self.cargo.spec.cargo_key,
          self.pool(),
        )
        .await
        .unwrap(),
        resource_kind: "Cargo".into(),
        resource_key: self.cargo.spec.cargo_key.clone(),
        node_name: mutex.node_key,
        owner_boot_id: uuid::Uuid::nil().to_string(),
        lock_device: 1,
        lock_inode: 1,
      };
      OperationDb::new(
        &owner,
        "Cargo",
        &self.cargo.spec.cargo_key,
        spec_key,
        action,
      )
    }

    async fn finish(&self, operation: &OperationDb) {
      assert!(
        CargoDb::finish_operation(
          operation,
          ObjPsStatusUpdate {
            actual: Some("start".into()),
            health: Some("healthy".into()),
            ..Default::default()
          },
          "completed",
          None,
          (),
          self.pool()
        )
        .await
        .unwrap()
      );
    }

    async fn node(&mut self, suffix: &str) -> String {
      let name = format!("{}-{suffix}", self.namespace);
      NodeDb::create_from(
        NodeDb {
          name: name.clone(),
          created_at: chrono::Utc::now().naive_utc(),
          endpoint: "http://127.0.0.1:0".into(),
          version: vars::VERSION.into(),
          metadata: None,
        },
        self.pool(),
      )
      .await
      .unwrap();
      self.nodes.push(name.clone());
      name
    }

    async fn cleanup(self) {
      let key = self.cargo.spec.cargo_key;
      let namespace = self.namespace;
      let nodes_to_delete = self.nodes;
      run_query_owned(&self.system.state.inner.pool, (), move |conn| {
        transaction(conn, |conn| {
          use crate::schema::{namespaces, nodes, processes};
          let identities = resource_incarnations::table
            .filter(resource_incarnations::resource_kind.eq("Cargo"))
            .filter(resource_incarnations::resource_key.eq(&key))
            .select(resource_incarnations::key)
            .load::<uuid::Uuid>(conn)?;
          diesel::delete(cargoes::table.find(&key)).execute(conn)?;
          diesel::delete(
            processes::table
              .filter(processes::kind.eq("cargo"))
              .filter(processes::kind_key.eq(&key)),
          )
          .execute(conn)?;
          diesel::delete(object_process_statuses::table.find(&key))
            .execute(conn)?;
          diesel::update(
            resource_incarnations::table
              .filter(resource_incarnations::key.eq_any(&identities)),
          )
          .set(
            resource_incarnations::current_operation_key.eq(None::<uuid::Uuid>),
          )
          .execute(conn)?;
          diesel::delete(
            operations::table
              .filter(operations::incarnation_key.eq_any(&identities)),
          )
          .execute(conn)?;
          diesel::delete(
            specs::table.filter(specs::incarnation_key.eq_any(&identities)),
          )
          .execute(conn)?;
          diesel::delete(
            resource_incarnations::table
              .filter(resource_incarnations::key.eq_any(&identities)),
          )
          .execute(conn)?;
          diesel::delete(
            distributed_mutexes::table
              .filter(distributed_mutexes::resource_kind.eq("Cargo"))
              .filter(distributed_mutexes::resource_key.eq(&key)),
          )
          .execute(conn)?;
          diesel::delete(namespaces::table.find(&namespace)).execute(conn)?;
          diesel::delete(
            nodes::table.filter(nodes::name.eq_any(&nodes_to_delete)),
          )
          .execute(conn)?;
          Ok(())
        })
      })
      .await
      .unwrap();
    }
  }

  #[ntex::test]
  async fn cargo_definition_and_failed_admission_leave_no_partial_revision() {
    let fixture = Fixture::new().await;
    let original = fixture.cargo.clone();
    let duplicate = fixture.replacement();
    let duplicate_key = duplicate.key;
    assert!(
      CargoDb::create_with_spec(
        &original.spec.cargo_key,
        "service",
        &fixture.namespace,
        duplicate,
        stopped(&original.spec.cargo_key),
        fixture.pool()
      )
      .await
      .is_err()
    );
    assert!(
      SpecDb::read_by_pk(&duplicate_key, fixture.pool())
        .await
        .is_err()
    );
    let replacement = fixture.replacement();
    let revision = replacement.key;
    let mut operation = fixture.operation(revision, "update").await;
    // Rejected by the shared acceptance guard after replacement insertion.
    operation.completed_at = Some(operation.accepted_at);
    assert!(
      CargoDb::accept_operation(
        &original,
        Some(replacement),
        operation.clone(),
        (),
        fixture.pool()
      )
      .await
      .is_err()
    );
    assert!(SpecDb::read_by_pk(&revision, fixture.pool()).await.is_err());
    assert!(
      OperationDb::get(operation.key, fixture.pool())
        .await
        .unwrap()
        .is_none()
    );
    let current =
      CargoDb::transform_read_by_pk(&original.spec.cargo_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(
      serde_json::to_value(&current).unwrap(),
      serde_json::to_value(&original).unwrap()
    );
    assert_eq!(
      CargoDb::history(&current.spec.cargo_key, fixture.pool())
        .await
        .unwrap()
        .len(),
      1
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn cargo_stale_operations_cannot_mutate_successor_or_recreated_identity()
   {
    let mut fixture = Fixture::new().await;
    let first = fixture.operation(fixture.cargo.spec.key, "start").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      first.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    fixture.finish(&first).await;
    fixture.cargo =
      CargoDb::transform_read_by_pk(&first.resource_key, fixture.pool())
        .await
        .unwrap();
    let replacement = fixture.replacement();
    let second = fixture.operation(replacement.key, "update").await;
    let accepted = CargoDb::accept_operation(
      &fixture.cargo,
      Some(replacement),
      second.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      !CargoDb::update_status_owned(
        &first,
        ObjPsStatusUpdate {
          actual: Some("fail".into()),
          ..Default::default()
        },
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      !CargoDb::complete_operation(&first, "failed", None, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      CargoDb::update_from_spec(
        &second.resource_key,
        &CargoSpec {
          name: "service".into(),
          ..Default::default()
        },
        vars::VERSION,
        fixture.pool()
      )
      .await
      .is_err()
    );
    assert!(
      CargoDb::finish_operation(
        &second,
        ObjPsStatusUpdate {
          actual: Some("fail".into()),
          prev_actual: Some("updating".into()),
          health: Some("unhealthy".into()),
          ..Default::default()
        },
        "failed",
        None,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    fixture.cargo =
      CargoDb::transform_read_by_pk(&second.resource_key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(fixture.cargo.spec.key, second.spec_key);
    assert_eq!(fixture.cargo.status.wanted, ObjPsStatusKind::Start);
    assert_eq!(fixture.cargo.status.actual, ObjPsStatusKind::Fail);
    assert_eq!(fixture.cargo.status.prev_actual, ObjPsStatusKind::Updating);
    let deletion = fixture.operation(accepted.cargo.spec.key, "delete").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      deletion.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      CargoDb::delete_operation(&deletion, (), fixture.pool())
        .await
        .unwrap()
    );
    let key = fixture.cargo.spec.cargo_key.clone();
    fixture.cargo = CargoDb::create_with_spec(
      &key,
      "service",
      &fixture.namespace,
      fixture.replacement(),
      stopped(&key),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      !CargoDb::delete_operation(&deletion, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      !CargoDb::validate_operation(&second, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      CargoDb::history_spec(&key, second.spec_key, fixture.pool())
        .await
        .is_err()
    );
    assert!(
      SpecDb::read_by_pk(&second.spec_key, fixture.pool())
        .await
        .is_ok()
    );
    assert_ne!(
      CargoDb::incarnation(&key, fixture.pool()).await.unwrap(),
      second.incarnation_key
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn cargo_remote_process_writes_require_assignment_and_retained_identity()
   {
    let mut fixture = Fixture::new().await;
    let remote = fixture.node("remote").await;
    let foreign = fixture.node("foreign").await;
    let operation = fixture.operation(fixture.cargo.spec.key, "start").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      operation.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    let replica = CargoReplicaDb::create(
      NewCargoReplicaDb {
        key: uuid::Uuid::new_v4(),
        cargo_key: operation.resource_key.clone(),
        ordinal: 0,
        node_name: Some(remote.clone()),
      },
      fixture.pool(),
    )
    .await
    .unwrap();
    let id = format!("{}-container", fixture.namespace);
    let mut process = ProcessPartial {
      key: id.clone(),
      name: id.clone(),
      kind: ProcessKind::Cargo,
      kind_key: operation.resource_key.clone(),
      node_name: remote,
      created_at: None,
      data: serde_json::json!({"Id":id,"Config":{"Labels":{
        "io.nanocl.kind":"cargo","io.nanocl.c":operation.resource_key,
        "io.nanocl.cargo.replica":replica.key,
        "io.nanocl.cargo.replica-ordinal":"0",
        "io.nanocl.cargo.incarnation":operation.incarnation_key,
        "io.nanocl.cargo.spec":operation.spec_key,
        "io.nanocl.cargo.operation":operation.key,
      }}}),
    };
    ProcessDb::create_owned(
      &process,
      Some(operation.owner()),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    process.node_name = foreign;
    process.key = format!("{id}-foreign");
    process.data["Id"] = serde_json::json!(process.key);
    assert!(
      ProcessDb::create_owned(
        &process,
        Some(operation.owner()),
        (),
        fixture.pool()
      )
      .await
      .is_err()
    );
    assert!(
      ProcessDb::update_owned(
        &id,
        ProcessUpdateDb {
          data: Some(serde_json::json!({"Id":id})),
          ..Default::default()
        },
        Some(operation.owner()),
        (),
        fixture.pool()
      )
      .await
      .is_err()
    );
    fixture.finish(&operation).await;
    fixture.cargo =
      CargoDb::transform_read_by_pk(&operation.resource_key, fixture.pool())
        .await
        .unwrap();
    let restart = fixture.operation(fixture.cargo.spec.key, "restart").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      restart.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    ProcessDb::update_owned(
      &id,
      ProcessUpdateDb {
        name: Some("retained-runtime".into()),
        ..Default::default()
      },
      Some(restart.owner()),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      ProcessDb::delete_owned(&id, Some(operation.owner()), (), fixture.pool())
        .await
        .is_err()
    );
    ProcessDb::delete_owned(&id, Some(restart.owner()), (), fixture.pool())
      .await
      .unwrap();
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn cargo_retained_bootstrap_cleanup_survives_promotion_and_new_operation()
   {
    let mut fixture = Fixture::new().await;
    let node = fixture.node("installer").await;
    let replica = CargoReplicaDb::create(
      NewCargoReplicaDb {
        key: uuid::Uuid::new_v4(),
        cargo_key: fixture.cargo.spec.cargo_key.clone(),
        ordinal: 0,
        node_name: Some(node.clone()),
      },
      fixture.pool(),
    )
    .await
    .unwrap();
    let id = format!("{}-installer", fixture.namespace);
    let process = ProcessPartial {
      key: id.clone(),
      name: id.clone(),
      kind: ProcessKind::Cargo,
      kind_key: fixture.cargo.spec.cargo_key.clone(),
      node_name: node,
      created_at: None,
      data: serde_json::json!({"Id":id,"Config":{"Labels":{
        "io.nanocl.kind":"cargo","io.nanocl.c":fixture.cargo.spec.cargo_key,
        "io.nanocl.cargo.replica":replica.key,
        "io.nanocl.cargo.replica-ordinal":"0",
      }}}),
    };
    ProcessDb::create_from(&process, fixture.pool())
      .await
      .unwrap();
    let mut mapping = NewCargoReplicaProcessDb::new(
      replica.key,
      "app",
      CargoReplicaProcessRole::App,
      true,
    );
    mapping.process_key = Some(id.clone());
    let mapping = CargoReplicaProcessDb::create(mapping, fixture.pool())
      .await
      .unwrap();
    let operation = fixture.operation(fixture.cargo.spec.key, "update").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      operation.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    CargoReplicaProcessDb::clear_process_owned(
      mapping.key,
      Some(operation.owner()),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    ProcessDb::validate_cargo_owned(&id, operation.owner(), (), fixture.pool())
      .await
      .unwrap();
    ProcessDb::update_owned(
      &id,
      ProcessUpdateDb {
        name: Some("retained-installer".into()),
        ..Default::default()
      },
      Some(operation.owner()),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    let mut wrong_ordinal = process.data.clone();
    wrong_ordinal["Config"]["Labels"]["io.nanocl.cargo.replica-ordinal"] =
      serde_json::json!("1");
    assert!(
      ProcessDb::update_owned(
        &id,
        ProcessUpdateDb {
          data: Some(wrong_ordinal),
          ..Default::default()
        },
        Some(operation.owner()),
        (),
        fixture.pool()
      )
      .await
      .is_err()
    );
    CargoReplicaProcessDb::set_processes_owned(
      vec![(mapping.key, Some(id.clone()), true)],
      Some(operation.owner()),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    CargoReplicaProcessDb::clear_process_owned(
      mapping.key,
      Some(operation.owner()),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    fixture.finish(&operation).await;
    fixture.cargo =
      CargoDb::transform_read_by_pk(&operation.resource_key, fixture.pool())
        .await
        .unwrap();
    let restart = fixture.operation(fixture.cargo.spec.key, "restart").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      restart.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      ProcessDb::validate_cargo_owned(
        &id,
        operation.owner(),
        (),
        fixture.pool()
      )
      .await
      .is_err()
    );
    // The old mapping stays empty: a new owner can still clean up the exact
    // installer ID through the unchanged durable replica identity.
    ProcessDb::validate_cargo_owned(&id, restart.owner(), (), fixture.pool())
      .await
      .unwrap();
    ProcessDb::delete_owned(&id, Some(restart.owner()), (), fixture.pool())
      .await
      .unwrap();
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn cargo_recreated_incarnation_rejects_retained_bootstrap_replica() {
    let mut fixture = Fixture::new().await;
    let node = fixture.node("installer").await;
    let key = fixture.cargo.spec.cargo_key.clone();
    let replica = CargoReplicaDb::create(
      NewCargoReplicaDb {
        key: uuid::Uuid::new_v4(),
        cargo_key: key.clone(),
        ordinal: 0,
        node_name: Some(node.clone()),
      },
      fixture.pool(),
    )
    .await
    .unwrap();
    let id = format!("{}-old-installer", fixture.namespace);
    let process = ProcessPartial {
      key: id.clone(),
      name: id.clone(),
      kind: ProcessKind::Cargo,
      kind_key: key.clone(),
      node_name: node.clone(),
      created_at: None,
      data: serde_json::json!({"Id":id,"Config":{"Labels":{
        "io.nanocl.kind":"cargo","io.nanocl.c":key,
        "io.nanocl.cargo.replica":replica.key,
        "io.nanocl.cargo.replica-ordinal":"0",
      }}}),
    };
    ProcessDb::create_from(&process, fixture.pool())
      .await
      .unwrap();
    let deletion = fixture.operation(fixture.cargo.spec.key, "delete").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      deletion.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    ProcessDb::validate_cargo_owned(&id, deletion.owner(), (), fixture.pool())
      .await
      .unwrap();
    // Model an old inventory row surviving deletion. The replica itself must
    // be removed by the live Cargo foreign key, so recreation cannot adopt it.
    assert!(
      CargoDb::delete_operation(&deletion, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      CargoReplicaDb::get(replica.key, fixture.pool())
        .await
        .is_err()
    );
    fixture.cargo = CargoDb::create_with_spec(
      &key,
      "service",
      &fixture.namespace,
      fixture.replacement(),
      stopped(&key),
      fixture.pool(),
    )
    .await
    .unwrap();
    CargoReplicaDb::create(
      NewCargoReplicaDb {
        key: uuid::Uuid::new_v4(),
        cargo_key: key.clone(),
        ordinal: 0,
        node_name: Some(node),
      },
      fixture.pool(),
    )
    .await
    .unwrap();
    let current = fixture.operation(fixture.cargo.spec.key, "start").await;
    CargoDb::accept_operation(
      &fixture.cargo,
      None,
      current.clone(),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      ProcessDb::validate_cargo_owned(
        &id,
        deletion.owner(),
        (),
        fixture.pool()
      )
      .await
      .is_err()
    );
    assert!(
      ProcessDb::validate_cargo_owned(&id, current.owner(), (), fixture.pool())
        .await
        .is_err()
    );
    assert!(
      ProcessDb::delete_owned(&id, Some(current.owner()), (), fixture.pool())
        .await
        .is_err()
    );
    fixture.cleanup().await;
  }
}
