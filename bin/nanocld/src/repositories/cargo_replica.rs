use std::collections::HashSet;

use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult};

use crate::{
  models::{
    CargoReplicaAssignment, CargoReplicaDb, CargoReplicaProcessDb,
    CargoReplicaProcessRole, CargoReplicaProcessUpdateDb, CargoReplicaUpdateDb,
    NewCargoReplicaDb, NewCargoReplicaProcessDb, Pool,
  },
  schema::{cargo_replica_processes, cargo_replicas, cargoes, nodes},
  utils::{
    self,
    container::{
      cargo_replica::{
        CargoReplicaReconcileError, placement_node_names, plan_reconciliation,
        validate_persisted_assignments,
      },
      generic::SelectionPlan,
    },
  },
};

use super::generic::RepositoryBase;

impl RepositoryBase for CargoReplicaDb {}
impl RepositoryBase for CargoReplicaProcessDb {}

async fn run_query<T, F>(
  pool: &Pool,
  operation: &'static str,
  query: F,
) -> IoResult<T>
where
  T: Send + 'static,
  F: FnOnce(&mut diesel::PgConnection) -> IoResult<T> + Send + 'static,
{
  let pool = pool.clone();
  ntex::rt::spawn_blocking(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    query(&mut conn)
  })
  .await
  .map_err(|err| IoError::interrupted(operation, err.to_string().as_str()))?
}

fn map_replica_error(error: diesel::result::Error) -> IoError {
  CargoReplicaDb::map_err(error).into()
}

fn map_process_error(error: diesel::result::Error) -> IoError {
  CargoReplicaProcessDb::map_err(error).into()
}

#[derive(Debug)]
enum CargoReplicaTransactionError {
  Reconcile(CargoReplicaReconcileError),
  Database(diesel::result::Error),
  #[cfg(test)]
  InjectedFailure {
    operation: &'static str,
    ordinal: i32,
  },
}

impl std::fmt::Display for CargoReplicaTransactionError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Reconcile(error) => error.fmt(f),
      Self::Database(error) => {
        write!(f, "Cargo replica reconciliation database failure: {error}")
      }
      #[cfg(test)]
      Self::InjectedFailure { operation, ordinal } => write!(
        f,
        "injected Cargo replica reconciliation failure after {operation} ordinal {ordinal}"
      ),
    }
  }
}

impl std::error::Error for CargoReplicaTransactionError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Reconcile(error) => Some(error),
      Self::Database(error) => Some(error),
      #[cfg(test)]
      Self::InjectedFailure { .. } => None,
    }
  }
}

impl From<CargoReplicaReconcileError> for CargoReplicaTransactionError {
  fn from(error: CargoReplicaReconcileError) -> Self {
    Self::Reconcile(error)
  }
}

impl From<diesel::result::Error> for CargoReplicaTransactionError {
  fn from(error: diesel::result::Error) -> Self {
    Self::Database(error)
  }
}

impl From<CargoReplicaTransactionError> for IoError {
  fn from(error: CargoReplicaTransactionError) -> Self {
    IoError::other("CargoReplicaReconcile", error.to_string().as_str())
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconcileMutation {
  Created(i32),
  Reassigned(i32),
  Deleted(i32),
}

fn reconcile_assignments_on_conn<F>(
  conn: &mut diesel::PgConnection,
  cargo_key: &str,
  selection: &SelectionPlan,
  hook: &mut F,
) -> Result<Vec<CargoReplicaAssignment>, CargoReplicaTransactionError>
where
  F: FnMut(ReconcileMutation) -> Result<(), CargoReplicaTransactionError>,
{
  cargoes::table
    .find(cargo_key)
    .select(cargoes::key)
    .for_update()
    .first::<String>(conn)?;

  let requested_nodes = placement_node_names(cargo_key, selection)?;
  let locked_nodes = nodes::table
    .filter(nodes::name.eq_any(&requested_nodes))
    .select(nodes::name)
    .order(nodes::name.asc())
    .for_update()
    .load::<String>(conn)?;
  if locked_nodes != requested_nodes {
    let locked_nodes = locked_nodes.into_iter().collect::<HashSet<_>>();
    let node_name = requested_nodes
      .into_iter()
      .find(|node_name| !locked_nodes.contains(node_name))
      .expect("different sorted node sets contain a missing requested node");
    return Err(
      CargoReplicaReconcileError::MissingPlacementNode {
        cargo_key: cargo_key.to_owned(),
        node_name,
      }
      .into(),
    );
  }

  let existing = cargo_replicas::table
    .filter(cargo_replicas::cargo_key.eq(cargo_key))
    .order(cargo_replicas::ordinal.asc())
    .select(CargoReplicaDb::as_select())
    .for_update()
    .load::<CargoReplicaDb>(conn)?;

  // Lock existing process mappings before the pure planner can authorize a
  // scale-down. The planner rejects mapped surplus rows before any mutation.
  let mapped_replica_keys = if existing.is_empty() {
    HashSet::new()
  } else {
    let replica_keys = existing
      .iter()
      .map(|replica| replica.key)
      .collect::<Vec<_>>();
    cargo_replica_processes::table
      .filter(cargo_replica_processes::replica_key.eq_any(replica_keys))
      .select(cargo_replica_processes::replica_key)
      .order(cargo_replica_processes::replica_key.asc())
      .for_update()
      .load::<uuid::Uuid>(conn)?
      .into_iter()
      .collect::<HashSet<_>>()
  };
  let plan =
    plan_reconciliation(cargo_key, selection, &existing, &mapped_replica_keys)?;

  for (key, ordinal, node_name) in &plan.node_updates {
    diesel::update(cargo_replicas::table.find(key))
      .set(CargoReplicaUpdateDb::node(node_name.as_deref()))
      .execute(conn)?;
    hook(ReconcileMutation::Reassigned(*ordinal))?;
  }
  for replica in &plan.creates {
    diesel::insert_into(cargo_replicas::table)
      .values(replica)
      .execute(conn)?;
    hook(ReconcileMutation::Created(replica.ordinal))?;
  }
  for replica in plan.deletes.iter().rev() {
    diesel::delete(cargo_replicas::table.find(replica.key)).execute(conn)?;
    hook(ReconcileMutation::Deleted(replica.ordinal))?;
  }

  let final_rows = cargo_replicas::table
    .filter(cargo_replicas::cargo_key.eq(cargo_key))
    .order(cargo_replicas::ordinal.asc())
    .select(CargoReplicaDb::as_select())
    .load::<CargoReplicaDb>(conn)?;
  let assignments =
    validate_persisted_assignments(cargo_key, selection, final_rows)?;
  if assignments != plan.assignments {
    return Err(
      CargoReplicaReconcileError::IncompleteAssignment {
        cargo_key: cargo_key.to_owned(),
        reason: "persisted assignments differ from the reconciliation plan",
      }
      .into(),
    );
  }
  Ok(assignments)
}

impl CargoReplicaDb {
  /// Atomically reconcile stable replica identities and node assignments for
  /// one Cargo, returning the committed source-of-truth assignments in ordinal
  /// order.
  pub(crate) async fn reconcile_assignments(
    cargo_key: &str,
    selection: &SelectionPlan,
    pool: &Pool,
  ) -> IoResult<Vec<CargoReplicaAssignment>> {
    Self::reconcile_assignments_with_hook(
      cargo_key,
      selection,
      pool,
      |_| Ok(()),
    )
    .await
  }

  async fn reconcile_assignments_with_hook<F>(
    cargo_key: &str,
    selection: &SelectionPlan,
    pool: &Pool,
    mut hook: F,
  ) -> IoResult<Vec<CargoReplicaAssignment>>
  where
    F: FnMut(ReconcileMutation) -> Result<(), CargoReplicaTransactionError>
      + Send
      + 'static,
  {
    let cargo_key = cargo_key.to_owned();
    let selection = selection.clone();
    run_query(
      pool,
      "Interrupted while reconciling Cargo replica assignments",
      move |conn| {
        conn
          .build_transaction()
          .serializable()
          .run::<_, CargoReplicaTransactionError, _>(|conn| {
            reconcile_assignments_on_conn(
              conn, &cargo_key, &selection, &mut hook,
            )
          })
          .map_err(IoError::from)
      },
    )
    .await
  }

  /// Create one stable Cargo replica identity.
  pub(crate) async fn create(
    replica: NewCargoReplicaDb,
    pool: &Pool,
  ) -> IoResult<Self> {
    run_query(
      pool,
      "Interrupted while creating Cargo replica",
      move |conn| {
        diesel::insert_into(cargo_replicas::table)
          .values(replica)
          .returning(Self::as_returning())
          .get_result(conn)
          .map_err(map_replica_error)
      },
    )
    .await
  }

  /// Get one Cargo replica by its stable UUID.
  pub(crate) async fn get(key: uuid::Uuid, pool: &Pool) -> IoResult<Self> {
    run_query(
      pool,
      "Interrupted while reading Cargo replica",
      move |conn| {
        cargo_replicas::table
          .find(key)
          .select(Self::as_select())
          .first(conn)
          .map_err(map_replica_error)
      },
    )
    .await
  }

  /// List all replicas for one Cargo in stable ordinal order.
  pub(crate) async fn list_by_cargo(
    cargo_key: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Self>> {
    let cargo_key = cargo_key.to_owned();
    run_query(
      pool,
      "Interrupted while listing Cargo replicas",
      move |conn| {
        cargo_replicas::table
          .filter(cargo_replicas::cargo_key.eq(cargo_key))
          .order(cargo_replicas::ordinal.asc())
          .select(Self::as_select())
          .load(conn)
          .map_err(map_replica_error)
      },
    )
    .await
  }

  /// Assign a replica to a node.
  pub(crate) async fn assign_node(
    key: uuid::Uuid,
    node_name: &str,
    pool: &Pool,
  ) -> IoResult<Self> {
    Self::set_node(key, Some(node_name), pool).await
  }

  /// Clear the current node assignment while preserving replica identity.
  pub(crate) async fn clear_node(
    key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<Self> {
    Self::set_node(key, None, pool).await
  }

  async fn set_node(
    key: uuid::Uuid,
    node_name: Option<&str>,
    pool: &Pool,
  ) -> IoResult<Self> {
    let update = CargoReplicaUpdateDb::node(node_name);
    run_query(
      pool,
      "Interrupted while assigning Cargo replica",
      move |conn| {
        diesel::update(cargo_replicas::table.find(key))
          .set(update)
          .returning(Self::as_returning())
          .get_result(conn)
          .map_err(map_replica_error)
      },
    )
    .await
  }

  /// Delete one Cargo replica and its logical process mappings.
  pub(crate) async fn delete(key: uuid::Uuid, pool: &Pool) -> IoResult<()> {
    run_query(
      pool,
      "Interrupted while deleting Cargo replica",
      move |conn| {
        diesel::delete(cargo_replicas::table.find(key))
          .execute(conn)
          .map(|_| ())
          .map_err(map_replica_error)
      },
    )
    .await
  }

  /// Delete every replica belonging to one Cargo.
  pub(crate) async fn delete_all_by_cargo(
    cargo_key: &str,
    pool: &Pool,
  ) -> IoResult<()> {
    let cargo_key = cargo_key.to_owned();
    run_query(
      pool,
      "Interrupted while deleting Cargo replicas",
      move |conn| {
        diesel::delete(
          cargo_replicas::table.filter(cargo_replicas::cargo_key.eq(cargo_key)),
        )
        .execute(conn)
        .map(|_| ())
        .map_err(map_replica_error)
      },
    )
    .await
  }
}

impl CargoReplicaProcessDb {
  /// Create one stable logical Cargo process mapping.
  pub(crate) async fn create(
    process: NewCargoReplicaProcessDb,
    pool: &Pool,
  ) -> IoResult<Self> {
    run_query(
      pool,
      "Interrupted while creating Cargo replica process",
      move |conn| {
        diesel::insert_into(cargo_replica_processes::table)
          .values(process)
          .returning(Self::as_returning())
          .get_result(conn)
          .map_err(map_process_error)
      },
    )
    .await
  }

  /// Get one logical process mapping by UUID.
  pub(crate) async fn get(key: uuid::Uuid, pool: &Pool) -> IoResult<Self> {
    run_query(
      pool,
      "Interrupted while reading Cargo replica process",
      move |conn| {
        cargo_replica_processes::table
          .find(key)
          .select(Self::as_select())
          .first(conn)
          .map_err(map_process_error)
      },
    )
    .await
  }

  /// List mappings in deterministic lowercase role and container-name order.
  pub(crate) async fn list_by_replica(
    replica_key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<Vec<Self>> {
    run_query(
      pool,
      "Interrupted while listing Cargo replica processes",
      move |conn| {
        cargo_replica_processes::table
          .filter(cargo_replica_processes::replica_key.eq(replica_key))
          .order((
            cargo_replica_processes::role.asc(),
            cargo_replica_processes::container_name.asc(),
          ))
          .select(Self::as_select())
          .load(conn)
          .map_err(map_process_error)
      },
    )
    .await
  }

  /// Find one mapping by its stable logical identity.
  pub(crate) async fn find(
    replica_key: uuid::Uuid,
    role: CargoReplicaProcessRole,
    container_name: &str,
    pool: &Pool,
  ) -> IoResult<Self> {
    let container_name = container_name.to_owned();
    run_query(
      pool,
      "Interrupted while finding Cargo replica process",
      move |conn| {
        cargo_replica_processes::table
          .filter(cargo_replica_processes::replica_key.eq(replica_key))
          .filter(cargo_replica_processes::role.eq(role))
          .filter(cargo_replica_processes::container_name.eq(container_name))
          .select(Self::as_select())
          .first(conn)
          .map_err(map_process_error)
      },
    )
    .await
  }

  /// Find a logical mapping by its currently attached concrete process.
  pub(crate) async fn find_by_process(
    process_key: &str,
    pool: &Pool,
  ) -> IoResult<Self> {
    let process_key = process_key.to_owned();
    run_query(
      pool,
      "Interrupted while finding Cargo replica process",
      move |conn| {
        cargo_replica_processes::table
          .filter(cargo_replica_processes::process_key.eq(process_key))
          .select(Self::as_select())
          .first(conn)
          .map_err(map_process_error)
      },
    )
    .await
  }

  /// Attach a concrete generic process to this logical mapping.
  pub(crate) async fn attach_process(
    key: uuid::Uuid,
    process_key: &str,
    pool: &Pool,
  ) -> IoResult<Self> {
    Self::set_process(key, Some(process_key), pool).await
  }

  /// Atomically replace the concrete-process attachments for a rollout.
  ///
  /// Every requested mapping is locked before any update. This keeps a
  /// multi-container replica from exposing a partially promoted generation if
  /// one attachment fails.
  pub(crate) async fn set_processes(
    attachments: Vec<(uuid::Uuid, Option<String>, bool)>,
    pool: &Pool,
  ) -> IoResult<Vec<Self>> {
    run_query(
      pool,
      "Interrupted while promoting Cargo replica processes",
      move |conn| {
        conn
          .transaction::<_, diesel::result::Error, _>(|conn| {
            let mut keys = attachments
              .iter()
              .map(|(key, _, _)| *key)
              .collect::<Vec<_>>();
            keys.sort_unstable();
            keys.dedup();
            let locked = cargo_replica_processes::table
              .filter(cargo_replica_processes::key.eq_any(&keys))
              .order(cargo_replica_processes::key.asc())
              .select(cargo_replica_processes::key)
              .for_update()
              .load::<uuid::Uuid>(conn)?;
            if locked != keys {
              return Err(diesel::result::Error::NotFound);
            }
            let now = chrono::Utc::now().naive_utc();
            for (key, process_key, essential) in &attachments {
              diesel::update(cargo_replica_processes::table.find(key))
                .set((
                  cargo_replica_processes::process_key.eq(process_key),
                  cargo_replica_processes::essential.eq(essential),
                  cargo_replica_processes::updated_at.eq(now),
                ))
                .execute(conn)?;
            }
            cargo_replica_processes::table
              .filter(cargo_replica_processes::key.eq_any(keys))
              .order(cargo_replica_processes::key.asc())
              .select(Self::as_select())
              .load(conn)
          })
          .map_err(map_process_error)
      },
    )
    .await
  }

  /// Clear a missing or deleted concrete process while preserving identity.
  pub(crate) async fn clear_process(
    key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<Self> {
    Self::set_process(key, None, pool).await
  }

  async fn set_process(
    key: uuid::Uuid,
    process_key: Option<&str>,
    pool: &Pool,
  ) -> IoResult<Self> {
    let update = CargoReplicaProcessUpdateDb::process(process_key);
    run_query(
      pool,
      "Interrupted while attaching Cargo replica process",
      move |conn| {
        diesel::update(cargo_replica_processes::table.find(key))
          .set(update)
          .returning(Self::as_returning())
          .get_result(conn)
          .map_err(map_process_error)
      },
    )
    .await
  }

  /// Delete one logical process mapping.
  pub(crate) async fn delete(key: uuid::Uuid, pool: &Pool) -> IoResult<()> {
    run_query(
      pool,
      "Interrupted while deleting Cargo replica process",
      move |conn| {
        diesel::delete(cargo_replica_processes::table.find(key))
          .execute(conn)
          .map(|_| ())
          .map_err(map_process_error)
      },
    )
    .await
  }
}

#[cfg(test)]
mod tests {
  use std::fmt::Debug;

  use bollard_next::{
    container::Config as DockerConfig, service::ContainerInspectResponse,
  };
  use nanocl_stubs::{
    cargo_spec::{CargoSpec, ContainerSpec},
    generic::ImagePullPolicy,
    process::{ProcessKind, ProcessPartial},
  };

  use crate::{
    models::{
      CargoDb, CargoObjCreateIn, NamespaceDb, NodeDb, ProcessDb, SystemState,
    },
    objects::generic::ObjCreate,
    repositories::generic::{
      RepositoryCreate, RepositoryDelByPk, RepositoryReadBy,
    },
    utils::{
      container::generic::{SelectionAssignment, SelectionPlan},
      tests::{TestSystem, gen_test_system},
    },
    vars,
  };

  use super::*;

  fn no_routes(_: &mut ntex::web::ServiceConfig) {}

  struct Fixture {
    system: TestSystem,
    namespace: String,
    cargo_key: String,
    node_name: String,
    cargo_keys: Vec<String>,
    node_names: Vec<String>,
    process_keys: Vec<String>,
  }

  impl Fixture {
    async fn new() -> Self {
      let system = gen_test_system(no_routes, vars::VERSION).await;
      let suffix = uuid::Uuid::new_v4().simple().to_string();
      let namespace = format!("replica-test-{suffix}");
      NamespaceDb::create_from(
        NamespaceDb::new(&namespace),
        &system.state.inner.pool,
      )
      .await
      .unwrap();
      let cargo_key = create_cargo(&system.state, &namespace, "primary").await;
      let node_name = format!("replica-node-{suffix}");
      create_node(&system.state, &node_name).await;
      Self {
        system,
        namespace,
        cargo_key: cargo_key.clone(),
        node_name: node_name.clone(),
        cargo_keys: vec![cargo_key],
        node_names: vec![node_name],
        process_keys: Vec::new(),
      }
    }

    fn pool(&self) -> &Pool {
      &self.system.state.inner.pool
    }

    async fn create_cargo(&mut self, name: &str) -> String {
      let key = create_cargo(&self.system.state, &self.namespace, name).await;
      self.cargo_keys.push(key.clone());
      key
    }

    async fn create_node(&mut self, name: &str) {
      create_node(&self.system.state, name).await;
      self.node_names.push(name.to_owned());
    }

    async fn create_process(
      &mut self,
      kind: ProcessKind,
      kind_key: &str,
    ) -> ProcessDb {
      let key =
        format!("replica-process-{}-{}", kind, uuid::Uuid::new_v4().simple());
      let process = ProcessPartial {
        key: key.clone(),
        name: key.clone(),
        kind,
        data: serde_json::to_value(ContainerInspectResponse::default())
          .unwrap(),
        node_name: self.node_name.clone(),
        kind_key: kind_key.to_owned(),
        created_at: None,
      };
      let process =
        ProcessDb::create_from(&process, self.pool()).await.unwrap();
      self.process_keys.push(key);
      process
    }

    async fn cleanup(self) {
      let pool = &self.system.state.inner.pool;
      for cargo_key in &self.cargo_keys {
        let _ = CargoReplicaDb::delete_all_by_cargo(cargo_key, pool).await;
      }
      for process_key in &self.process_keys {
        let _ = ProcessDb::del_by_pk(process_key, pool).await;
      }
      for cargo_key in &self.cargo_keys {
        let _ = CargoDb::clear_by_pk(cargo_key, pool).await;
      }
      for node_name in &self.node_names {
        let _ = NodeDb::del_by_pk(node_name, pool).await;
      }
      let _ = NamespaceDb::del_by_pk(&self.namespace, pool).await;
      self.system.state.wait_event_loop().await;
    }
  }

  async fn create_cargo(
    state: &SystemState,
    namespace: &str,
    name: &str,
  ) -> String {
    let cargo = CargoDb::fn_create_obj(
      &CargoObjCreateIn {
        namespace: namespace.to_owned(),
        spec: CargoSpec {
          name: name.to_owned(),
          containers: vec![ContainerSpec {
            name: "main".to_owned(),
            essential: true,
            secrets: Vec::new(),
            image_pull_secret: None,
            image_pull_policy: ImagePullPolicy::IfNotPresent,
            container_config: DockerConfig {
              image: Some("example/app:1".to_owned()),
              ..Default::default()
            },
          }],
          ..Default::default()
        },
        version: vars::VERSION.to_owned(),
      },
      state,
    )
    .await
    .unwrap();
    cargo.spec.cargo_key
  }

  async fn create_node(state: &SystemState, name: &str) {
    NodeDb::create_from(
      NodeDb {
        name: name.to_owned(),
        created_at: chrono::Utc::now().naive_utc(),
        endpoint: "tcp://127.0.0.1:0".to_owned(),
        version: vars::VERSION.to_owned(),
        metadata: None,
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
  }

  fn assert_error_kind<T: Debug>(
    result: IoResult<T>,
    expected: std::io::ErrorKind,
  ) {
    let error = result.unwrap_err();
    assert_eq!(error.inner.kind(), expected, "{error}");
  }

  fn selection(
    total_replicas: usize,
    assignments: &[(&str, usize)],
  ) -> SelectionPlan {
    SelectionPlan {
      total_replicas,
      assignments: assignments
        .iter()
        .map(|(node_name, replicas)| SelectionAssignment {
          node: NodeDb {
            name: (*node_name).to_owned(),
            created_at: chrono::Utc::now().naive_utc(),
            endpoint: "tcp://127.0.0.1:0".to_owned(),
            version: vars::VERSION.to_owned(),
            metadata: None,
          },
          replicas: *replicas,
        })
        .collect(),
    }
  }

  fn assignment_nodes(
    assignments: &[CargoReplicaAssignment],
  ) -> Vec<(i32, Option<&str>)> {
    assignments
      .iter()
      .map(|assignment| (assignment.ordinal, assignment.node_name.as_deref()))
      .collect()
  }

  #[ntex::test]
  async fn replica_crud_order_and_constraints() {
    let fixture = Fixture::new().await;
    let pool = fixture.pool();
    let replica_two = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&fixture.cargo_key, 2),
      pool,
    )
    .await
    .unwrap();
    let replica_zero = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&fixture.cargo_key, 0),
      pool,
    )
    .await
    .unwrap();
    let replica_one = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&fixture.cargo_key, 1),
      pool,
    )
    .await
    .unwrap();

    assert_eq!(
      CargoReplicaDb::get(replica_one.key, pool).await.unwrap(),
      replica_one
    );
    let replicas = CargoReplicaDb::list_by_cargo(&fixture.cargo_key, pool)
      .await
      .unwrap();
    assert_eq!(
      replicas.iter().map(|item| item.ordinal).collect::<Vec<_>>(),
      [0, 1, 2]
    );

    assert_error_kind(
      CargoReplicaDb::create(
        NewCargoReplicaDb::new(&fixture.cargo_key, replica_zero.ordinal),
        pool,
      )
      .await,
      std::io::ErrorKind::AlreadyExists,
    );
    assert_error_kind(
      CargoReplicaDb::create(
        NewCargoReplicaDb::new(&fixture.cargo_key, -1),
        pool,
      )
      .await,
      std::io::ErrorKind::InvalidData,
    );

    CargoReplicaDb::delete(replica_two.key, pool).await.unwrap();
    assert_error_kind(
      CargoReplicaDb::get(replica_two.key, pool).await,
      std::io::ErrorKind::NotFound,
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn replica_assignment_and_foreign_key_actions() {
    let mut fixture = Fixture::new().await;
    let replica = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&fixture.cargo_key, 0),
      fixture.pool(),
    )
    .await
    .unwrap();

    let assigned = CargoReplicaDb::assign_node(
      replica.key,
      &fixture.node_name,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(
      assigned.node_name.as_deref(),
      Some(fixture.node_name.as_str())
    );
    let cleared = CargoReplicaDb::clear_node(replica.key, fixture.pool())
      .await
      .unwrap();
    assert!(cleared.node_name.is_none());
    CargoReplicaDb::assign_node(
      replica.key,
      &fixture.node_name,
      fixture.pool(),
    )
    .await
    .unwrap();
    NodeDb::del_by_pk(&fixture.node_name, fixture.pool())
      .await
      .unwrap();
    assert!(
      CargoReplicaDb::get(replica.key, fixture.pool())
        .await
        .unwrap()
        .node_name
        .is_none()
    );

    let mapping = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        replica.key,
        "web",
        CargoReplicaProcessRole::App,
        true,
      ),
      fixture.pool(),
    )
    .await
    .unwrap();
    CargoDb::del_by_pk(&fixture.cargo_key, fixture.pool())
      .await
      .unwrap();
    assert_error_kind(
      CargoReplicaDb::get(replica.key, fixture.pool()).await,
      std::io::ErrorKind::NotFound,
    );
    assert_error_kind(
      CargoReplicaProcessDb::get(mapping.key, fixture.pool()).await,
      std::io::ErrorKind::NotFound,
    );

    let cargo_a = fixture.create_cargo("delete-all").await;
    let cargo_b = fixture.create_cargo("preserve").await;
    let replica_a = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&cargo_a, 0),
      fixture.pool(),
    )
    .await
    .unwrap();
    let replica_b = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&cargo_b, 0),
      fixture.pool(),
    )
    .await
    .unwrap();
    CargoReplicaDb::delete_all_by_cargo(&cargo_a, fixture.pool())
      .await
      .unwrap();
    assert_error_kind(
      CargoReplicaDb::get(replica_a.key, fixture.pool()).await,
      std::io::ErrorKind::NotFound,
    );
    assert_eq!(
      CargoReplicaDb::get(replica_b.key, fixture.pool())
        .await
        .unwrap()
        .cargo_key,
      cargo_b
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn logical_mapping_crud_roles_order_and_constraints() {
    let fixture = Fixture::new().await;
    let pool = fixture.pool();
    let replica = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&fixture.cargo_key, 0),
      pool,
    )
    .await
    .unwrap();
    let init = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        replica.key,
        "migrate",
        CargoReplicaProcessRole::Init,
        true,
      ),
      pool,
    )
    .await
    .unwrap();
    let web = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        replica.key,
        "web",
        CargoReplicaProcessRole::App,
        false,
      ),
      pool,
    )
    .await
    .unwrap();
    let sandbox = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        replica.key,
        "_sandbox",
        CargoReplicaProcessRole::Sandbox,
        true,
      ),
      pool,
    )
    .await
    .unwrap();
    let api = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        replica.key,
        "api",
        CargoReplicaProcessRole::App,
        true,
      ),
      pool,
    )
    .await
    .unwrap();

    assert_eq!(
      CargoReplicaProcessDb::get(sandbox.key, pool).await.unwrap(),
      sandbox
    );
    assert_eq!(
      CargoReplicaProcessDb::find(
        replica.key,
        CargoReplicaProcessRole::Init,
        "migrate",
        pool,
      )
      .await
      .unwrap()
      .key,
      init.key
    );
    let mappings = CargoReplicaProcessDb::list_by_replica(replica.key, pool)
      .await
      .unwrap();
    assert_eq!(
      mappings
        .iter()
        .map(|item| (item.role, item.container_name.as_str()))
        .collect::<Vec<_>>(),
      [
        (CargoReplicaProcessRole::App, "api"),
        (CargoReplicaProcessRole::App, "web"),
        (CargoReplicaProcessRole::Init, "migrate"),
        (CargoReplicaProcessRole::Sandbox, "_sandbox"),
      ]
    );
    let raw_roles =
      run_query(pool, "Reading raw Cargo process roles", move |conn| {
        cargo_replica_processes::table
          .filter(cargo_replica_processes::replica_key.eq(replica.key))
          .order(cargo_replica_processes::role.asc())
          .select(cargo_replica_processes::role)
          .load::<String>(conn)
          .map_err(map_process_error)
      })
      .await
      .unwrap();
    assert_eq!(raw_roles, ["app", "app", "init", "sandbox"]);

    assert_error_kind(
      CargoReplicaProcessDb::create(
        NewCargoReplicaProcessDb::new(
          replica.key,
          "web",
          CargoReplicaProcessRole::App,
          true,
        ),
        pool,
      )
      .await,
      std::io::ErrorKind::AlreadyExists,
    );
    let constraint_replica = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&fixture.cargo_key, 1),
      pool,
    )
    .await
    .unwrap();
    assert_error_kind(
      CargoReplicaProcessDb::create(
        NewCargoReplicaProcessDb::new(
          constraint_replica.key,
          "_sandbox",
          CargoReplicaProcessRole::Sandbox,
          false,
        ),
        pool,
      )
      .await,
      std::io::ErrorKind::InvalidData,
    );
    assert_error_kind(
      CargoReplicaProcessDb::create(
        NewCargoReplicaProcessDb::new(
          replica.key,
          "prepare",
          CargoReplicaProcessRole::Init,
          false,
        ),
        pool,
      )
      .await,
      std::io::ErrorKind::InvalidData,
    );
    assert_error_kind(
      run_query(pool, "Inserting invalid Cargo process role", move |conn| {
        diesel::insert_into(cargo_replica_processes::table)
          .values((
            cargo_replica_processes::key.eq(uuid::Uuid::new_v4()),
            cargo_replica_processes::replica_key.eq(replica.key),
            cargo_replica_processes::container_name.eq("sidecar"),
            cargo_replica_processes::role.eq("sidecar"),
            cargo_replica_processes::essential.eq(true),
          ))
          .execute(conn)
          .map_err(map_process_error)
      })
      .await,
      std::io::ErrorKind::InvalidData,
    );

    CargoReplicaProcessDb::delete(web.key, pool).await.unwrap();
    assert_error_kind(
      CargoReplicaProcessDb::get(web.key, pool).await,
      std::io::ErrorKind::NotFound,
    );
    assert_eq!(api.container_name, "api");
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn process_attachment_cascade_and_generic_process_regression() {
    let mut fixture = Fixture::new().await;
    let replica = CargoReplicaDb::create(
      NewCargoReplicaDb::new(&fixture.cargo_key, 0),
      fixture.pool(),
    )
    .await
    .unwrap();
    let web = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        replica.key,
        "web",
        CargoReplicaProcessRole::App,
        true,
      ),
      fixture.pool(),
    )
    .await
    .unwrap();
    let worker = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        replica.key,
        "worker",
        CargoReplicaProcessRole::App,
        true,
      ),
      fixture.pool(),
    )
    .await
    .unwrap();
    let cargo_key = fixture.cargo_key.clone();
    let process = fixture.create_process(ProcessKind::Cargo, &cargo_key).await;

    let attached = CargoReplicaProcessDb::attach_process(
      web.key,
      &process.key,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(attached.process_key.as_deref(), Some(process.key.as_str()));
    assert_eq!(
      CargoReplicaProcessDb::find_by_process(&process.key, fixture.pool())
        .await
        .unwrap()
        .key,
      web.key
    );
    assert!(
      CargoReplicaProcessDb::clear_process(web.key, fixture.pool())
        .await
        .unwrap()
        .process_key
        .is_none()
    );
    CargoReplicaProcessDb::attach_process(
      web.key,
      &process.key,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_error_kind(
      CargoReplicaProcessDb::attach_process(
        worker.key,
        &process.key,
        fixture.pool(),
      )
      .await,
      std::io::ErrorKind::AlreadyExists,
    );
    ProcessDb::del_by_pk(&process.key, fixture.pool())
      .await
      .unwrap();
    assert!(
      CargoReplicaProcessDb::get(web.key, fixture.pool())
        .await
        .unwrap()
        .process_key
        .is_none()
    );

    CargoReplicaDb::delete(replica.key, fixture.pool())
      .await
      .unwrap();
    assert_error_kind(
      CargoReplicaProcessDb::get(web.key, fixture.pool()).await,
      std::io::ErrorKind::NotFound,
    );
    assert_error_kind(
      CargoReplicaProcessDb::get(worker.key, fixture.pool()).await,
      std::io::ErrorKind::NotFound,
    );

    for (kind, kind_key) in
      [(ProcessKind::Job, "job-key"), (ProcessKind::Vm, "vm-key")]
    {
      let process = fixture.create_process(kind, kind_key).await;
      let stored = ProcessDb::read_by_pk(&process.key, fixture.pool())
        .await
        .unwrap();
      assert_eq!(stored.kind, process.kind);
      assert_eq!(stored.kind_key, kind_key);
      assert_eq!(stored.node_name, fixture.node_name);
      assert!(
        CargoReplicaProcessDb::find_by_process(&process.key, fixture.pool())
          .await
          .is_err()
      );
    }
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn reconcile_stable_identity_scale_up_down_and_order() {
    let mut fixture = Fixture::new().await;
    let node_a = fixture.node_name.clone();
    let node_b = format!("{}-b", fixture.node_name);
    fixture.create_node(&node_b).await;

    let initial_cargo = fixture.create_cargo("initial-contiguous").await;
    let initial_three = CargoReplicaDb::reconcile_assignments(
      &initial_cargo,
      &selection(3, &[(node_a.as_str(), 3)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(
      initial_three
        .iter()
        .map(|assignment| assignment.ordinal)
        .collect::<Vec<_>>(),
      [0, 1, 2],
      "the first reconciliation must create every contiguous desired ordinal"
    );

    let first = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(1, &[(node_a.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(assignment_nodes(&first), [(0, Some(node_a.as_str()))]);
    let ordinal_zero_key = first[0].replica_key;
    let first_rows =
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap();

    let identical = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(1, &[(node_a.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(identical, first);
    assert_eq!(
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap(),
      first_rows,
      "an identical reconciliation must not rewrite timestamps"
    );

    let scaled = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(3, &[(node_b.as_str(), 1), (node_a.as_str(), 2)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(
      assignment_nodes(&scaled),
      [
        (0, Some(node_a.as_str())),
        (1, Some(node_a.as_str())),
        (2, Some(node_b.as_str()))
      ]
    );
    assert_eq!(scaled[0].replica_key, ordinal_zero_key);
    assert_ne!(scaled[1].replica_key, ordinal_zero_key);
    assert_ne!(scaled[2].replica_key, ordinal_zero_key);
    assert_ne!(scaled[1].replica_key, scaled[2].replica_key);

    let scaled_rows =
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap();
    let reordered = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(3, &[(node_a.as_str(), 2), (node_b.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(reordered, scaled);
    assert_eq!(
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap(),
      scaled_rows
    );

    for _ in 0..3 {
      let repeated = CargoReplicaDb::reconcile_assignments(
        &fixture.cargo_key,
        &selection(3, &[(node_b.as_str(), 1), (node_a.as_str(), 2)]),
        fixture.pool(),
      )
      .await
      .unwrap();
      assert_eq!(repeated, scaled);
      let ordinals = repeated
        .iter()
        .map(|assignment| assignment.ordinal)
        .collect::<HashSet<_>>();
      assert_eq!(ordinals.len(), 3);
      assert!(ordinals.contains(&0));
      assert!(ordinals.contains(&1));
      assert!(ordinals.contains(&2));
    }

    let ordinal_one_key = scaled[1].replica_key;
    let ordinal_two_key = scaled[2].replica_key;
    let reduced = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(1, &[(node_a.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(reduced.len(), 1);
    assert_eq!(reduced[0].ordinal, 0);
    assert_eq!(reduced[0].replica_key, ordinal_zero_key);
    assert_error_kind(
      CargoReplicaDb::get(ordinal_one_key, fixture.pool()).await,
      std::io::ErrorKind::NotFound,
    );
    assert_error_kind(
      CargoReplicaDb::get(ordinal_two_key, fixture.pool()).await,
      std::io::ErrorKind::NotFound,
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn reconcile_persists_partial_assignments_and_reuses_null_rows() {
    let mut fixture = Fixture::new().await;
    let node_a = fixture.node_name.clone();
    let node_b = format!("{}-b", fixture.node_name);
    fixture.create_node(&node_b).await;

    let initial = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(3, &[(node_a.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(
      assignment_nodes(&initial),
      [(0, Some(node_a.as_str())), (1, None), (2, None)]
    );
    let initial_rows =
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap();

    let repeated = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(3, &[(node_a.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(repeated, initial);
    assert_eq!(
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap(),
      initial_rows,
      "repeated partial placement must not rewrite durable rows"
    );

    let filled = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(3, &[(node_a.as_str(), 1), (node_b.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(
      assignment_nodes(&filled),
      [
        (0, Some(node_a.as_str())),
        (1, Some(node_b.as_str())),
        (2, None)
      ]
    );
    assert_eq!(
      filled
        .iter()
        .map(|assignment| assignment.replica_key)
        .collect::<Vec<_>>(),
      initial
        .iter()
        .map(|assignment| assignment.replica_key)
        .collect::<Vec<_>>(),
      "new capacity must assign an existing null replica"
    );

    let reduced_capacity = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(3, &[(node_a.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(
      assignment_nodes(&reduced_capacity),
      [(0, Some(node_a.as_str())), (1, None), (2, None)]
    );
    assert_eq!(
      reduced_capacity
        .iter()
        .map(|assignment| assignment.replica_key)
        .collect::<Vec<_>>(),
      initial
        .iter()
        .map(|assignment| assignment.replica_key)
        .collect::<Vec<_>>()
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn reconcile_scale_down_mapping_conflict_is_atomic() {
    let mut fixture = Fixture::new().await;
    let node_a = fixture.node_name.clone();
    let node_b = format!("{}-b", fixture.node_name);
    fixture.create_node(&node_b).await;
    let assignments = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(3, &[(node_a.as_str(), 3)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    let mapping = CargoReplicaProcessDb::create(
      NewCargoReplicaProcessDb::new(
        assignments[2].replica_key,
        "web",
        CargoReplicaProcessRole::App,
        true,
      ),
      fixture.pool(),
    )
    .await
    .unwrap();
    let before =
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap();
    let mapping_before =
      CargoReplicaProcessDb::get(mapping.key, fixture.pool())
        .await
        .unwrap();

    let error = CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(1, &[(node_b.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert_eq!(error.context(), Some("CargoReplicaReconcile"));
    assert!(error.to_string().contains(&fixture.cargo_key));
    assert!(error.to_string().contains("ordinal 2"));
    assert_eq!(
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap(),
      before
    );
    assert_eq!(
      CargoReplicaProcessDb::get(mapping.key, fixture.pool())
        .await
        .unwrap(),
      mapping_before
    );

    CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(4, &[(node_a.as_str(), 3), (node_b.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(
      CargoReplicaProcessDb::get(mapping.key, fixture.pool())
        .await
        .unwrap(),
      mapping_before,
      "successful scheduling must not alter existing process mappings"
    );
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn reconcile_injected_failures_roll_back_every_mutation_kind() {
    let mut fixture = Fixture::new().await;
    let node_a = fixture.node_name.clone();
    let node_b = format!("{}-b", fixture.node_name);
    fixture.create_node(&node_b).await;

    let create_error = CargoReplicaDb::reconcile_assignments_with_hook(
      &fixture.cargo_key,
      &selection(1, &[(node_a.as_str(), 1)]),
      fixture.pool(),
      |mutation| match mutation {
        ReconcileMutation::Created(ordinal) => {
          Err(CargoReplicaTransactionError::InjectedFailure {
            operation: "creating",
            ordinal,
          })
        }
        _ => Ok(()),
      },
    )
    .await;
    assert_error_kind(create_error, std::io::ErrorKind::Other);
    assert!(
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap()
        .is_empty()
    );

    CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(1, &[(node_a.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    let before_update =
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap();
    let update_error = CargoReplicaDb::reconcile_assignments_with_hook(
      &fixture.cargo_key,
      &selection(1, &[(node_b.as_str(), 1)]),
      fixture.pool(),
      |mutation| match mutation {
        ReconcileMutation::Reassigned(ordinal) => {
          Err(CargoReplicaTransactionError::InjectedFailure {
            operation: "reassigning",
            ordinal,
          })
        }
        _ => Ok(()),
      },
    )
    .await;
    assert_error_kind(update_error, std::io::ErrorKind::Other);
    assert_eq!(
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap(),
      before_update
    );

    CargoReplicaDb::reconcile_assignments(
      &fixture.cargo_key,
      &selection(2, &[(node_a.as_str(), 1), (node_b.as_str(), 1)]),
      fixture.pool(),
    )
    .await
    .unwrap();
    let before_delete =
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap();
    let delete_error = CargoReplicaDb::reconcile_assignments_with_hook(
      &fixture.cargo_key,
      &selection(1, &[(node_b.as_str(), 1)]),
      fixture.pool(),
      |mutation| match mutation {
        ReconcileMutation::Deleted(ordinal) => {
          Err(CargoReplicaTransactionError::InjectedFailure {
            operation: "deleting",
            ordinal,
          })
        }
        _ => Ok(()),
      },
    )
    .await;
    assert_error_kind(delete_error, std::io::ErrorKind::Other);
    assert_eq!(
      CargoReplicaDb::list_by_cargo(&fixture.cargo_key, fixture.pool())
        .await
        .unwrap(),
      before_delete,
      "the reassignment and deletion must both roll back"
    );
    fixture.cleanup().await;
  }
}
