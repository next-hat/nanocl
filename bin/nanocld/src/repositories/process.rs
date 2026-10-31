use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  process::{Process, ProcessPartial},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, OperationOwner, Pool, ProcessDb, ProcessUpdateDb},
  schema::processes,
};

use super::generic::*;

impl RepositoryBase for ProcessDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "processes.key")),
      ("name", (ColumnType::Text, "processes.name")),
      ("kind", (ColumnType::Text, "processes.kind")),
      ("node_name", (ColumnType::Text, "processes.node_name")),
      ("kind_key", (ColumnType::Text, "processes.kind_key")),
      ("data", (ColumnType::Json, "processes.data")),
      (
        "created_at",
        (ColumnType::Timestamptz, "processes.created_at"),
      ),
    ])
  }
}

impl RepositoryCreate for ProcessDb {}

impl RepositoryUpdate for ProcessDb {
  type UpdateItem = ProcessUpdateDb;
}

impl RepositoryDelByPk for ProcessDb {}

/// Implement delete_by_pk and delete_by_id for ProcessDb
impl RepositoryDelBy for ProcessDb {
  fn gen_del_query(
    filter: &GenericFilter,
  ) -> diesel::query_builder::BoxedDeleteStatement<
    'static,
    diesel::pg::Pg,
    <Self as diesel::associations::HasTable>::Table,
  >
  where
    Self: diesel::associations::HasTable,
  {
    let mut query = diesel::delete(processes::table).into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns)
  }
}

impl RepositoryReadBy for ProcessDb {
  type Output = ProcessDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::pg::PgConnection,
    Self::Output,
  > {
    let mut query = processes::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(processes::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for ProcessDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = processes::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for ProcessDb {
  type NewOutput = Process;

  fn transform(input: ProcessDb) -> IoResult<Self::NewOutput> {
    input.try_into()
  }
}

impl ProcessDb {
  fn belongs_to_operation(&self, owner: &OperationOwner) -> bool {
    // ProcessKind's stored spelling differs from the resource kind used by
    // operations. Keep this conversion exact instead of accepting arbitrary case.
    let resource_kind = match self.kind.as_str() {
      "vm" => "Vm",
      "cargo" => "Cargo",
      "job" => "Job",
      _ => return false,
    };
    resource_kind == owner.resource_kind
      && self.kind_key == owner.resource_key
      && self.node_name == owner.node_name
  }

  /// Cargo workers may execute on an assigned node other than the coordinator.
  /// Keep the coordinator owner exact and prove process membership in SQL.
  pub(super) fn validate_owned_process(
    &self,
    conn: &mut PgConnection,
    owner: &OperationOwner,
    creating: bool,
  ) -> super::operation::TransactionResult<()> {
    use crate::{
      models::{CargoDb, CargoReplicaDb, OperationDb, SpecDb},
      schema::{cargo_replicas, cargoes, operations, specs},
      utils::container::{cargo, cargo_compiler},
    };
    let reject = || {
      super::operation::conflict(
        "Process does not belong to its operation owner",
      )
    };
    if owner.resource_kind != "Cargo" {
      return if self.belongs_to_operation(owner) {
        Ok(())
      } else {
        Err(reject())
      };
    }
    let live = cargoes::table
      .find(&owner.resource_key)
      .for_update()
      .first::<CargoDb>(conn)?;
    if self.kind != "cargo"
      || self.kind_key != owner.resource_key
      || live.incarnation_key != owner.incarnation_key
      || self.data.get("Id").and_then(serde_json::Value::as_str)
        != Some(self.key.as_str())
    {
      return Err(reject());
    }
    let labels = self
      .data
      .get("Config")
      .and_then(|config| config.get("Labels"))
      .and_then(serde_json::Value::as_object)
      .ok_or_else(reject)?;
    let label = |key: &str| labels.get(key).and_then(serde_json::Value::as_str);
    if label(cargo_compiler::LABEL_KIND) != Some("cargo")
      || label(cargo_compiler::LABEL_CARGO_KEY)
        != Some(owner.resource_key.as_str())
    {
      return Err(reject());
    }
    let parse = |value: Option<&str>| {
      value
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .ok_or_else(reject)
    };
    let replica = cargo_replicas::table
      .find(parse(label(cargo_compiler::LABEL_REPLICA))?)
      .for_update()
      .select(CargoReplicaDb::as_select())
      .first::<CargoReplicaDb>(conn)?;
    let ordinal = label(cargo_compiler::LABEL_REPLICA_ORDINAL)
      .and_then(|value| value.parse::<i32>().ok())
      .ok_or_else(reject)?;
    if replica.cargo_key != self.kind_key
      || ordinal < 0
      || replica.ordinal != ordinal
      || !replica.node_name.as_deref().is_some_and(|node| {
        node == self.node_name
          || (node == "init-test.nanocl.io"
            && self.node_name == owner.node_name)
      })
    {
      return Err(reject());
    }
    let identity_labels = [
      cargo::LABEL_INCARNATION,
      cargo::LABEL_SPEC,
      cargo::LABEL_OPERATION,
    ];
    if identity_labels.iter().all(|key| !labels.contains_key(*key)) {
      // Installer containers predate operation labels. Their durable replica
      // UUID survives promotion but is deleted with this Cargo incarnation.
      // New runtime creation still requires the exact accepted operation.
      return if creating { Err(reject()) } else { Ok(()) };
    }
    let incarnation = parse(label(cargo::LABEL_INCARNATION))?;
    let spec_key = parse(label(cargo::LABEL_SPEC))?;
    let operation_key = parse(label(cargo::LABEL_OPERATION))?;
    if incarnation != owner.incarnation_key
      || (creating && operation_key != owner.key)
    {
      return Err(reject());
    }
    let operation = operations::table
      .find(operation_key)
      .first::<OperationDb>(conn)?;
    let spec = specs::table.find(spec_key).first::<SpecDb>(conn)?;
    if operation.incarnation_key != incarnation
      || operation.resource_kind != "Cargo"
      || operation.resource_key != self.kind_key
      || operation.spec_key != spec_key
      || spec.incarnation_key != Some(incarnation)
      || spec.kind_name != "Cargo"
      || spec.kind_key != self.kind_key
      || (creating && live.spec_key != spec_key)
    {
      return Err(reject());
    }
    Ok(())
  }

  pub(crate) async fn create_owned<G: Send + 'static>(
    item: &ProcessPartial,
    owner: Option<OperationOwner>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<Self> {
    let item = Self::from(item);
    let expected_owner = owner.clone();
    super::operation::run_mutation_owned(pool, keepalive, owner, move |conn| {
      if let Some(owner) = &expected_owner {
        item.validate_owned_process(conn, owner, true)?;
      }
      Ok(
        diesel::insert_into(processes::table)
          .values(&item)
          .get_result(conn)?,
      )
    })
    .await
  }

  pub(crate) async fn update_owned<G: Send + 'static>(
    key: &str,
    update: ProcessUpdateDb,
    owner: Option<OperationOwner>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<Self> {
    let key = key.to_owned();
    if update.key.as_ref().is_some_and(|updated| updated != &key) {
      return Err(
        super::operation::conflict(
          "Owned process update cannot change its immutable ID",
        )
        .into(),
      );
    }
    let expected_owner = owner.clone();
    super::operation::run_mutation_owned(pool, keepalive, owner, move |conn| {
      let process = processes::table
        .find(&key)
        .for_update()
        .first::<Self>(conn)?;
      if let Some(owner) = &expected_owner {
        process.validate_owned_process(conn, owner, false)?;
      }
      let process = diesel::update(processes::table.find(&key))
        .set(&update)
        .get_result::<Self>(conn)?;
      if let Some(owner) = &expected_owner {
        process.validate_owned_process(conn, owner, false)?;
      }
      Ok(process)
    })
    .await
  }

  /// Verify the exact process before an external Cargo runtime mutation.
  pub(crate) async fn validate_cargo_owned<G: Send + 'static>(
    key: &str,
    owner: OperationOwner,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<()> {
    if owner.resource_kind != "Cargo" {
      return Err(
        super::operation::conflict(
          "Cargo process validation has no exact operation owner",
        )
        .into(),
      );
    }
    let key = key.to_owned();
    let expected_owner = owner.clone();
    super::operation::run_mutation_owned(
      pool,
      keepalive,
      Some(owner),
      move |conn| {
        let process = processes::table
          .find(&key)
          .for_update()
          .first::<Self>(conn)?;
        process.validate_owned_process(conn, &expected_owner, false)
      },
    )
    .await
  }

  pub(crate) async fn delete_owned<G: Send + 'static>(
    key: &str,
    owner: Option<OperationOwner>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<()> {
    let key = key.to_owned();
    let expected_owner = owner.clone();
    super::operation::run_mutation_owned(pool, keepalive, owner, move |conn| {
      if let Some(owner) = &expected_owner
        && let Some(process) =
          processes::table.find(&key).first::<Self>(conn).optional()?
      {
        process.validate_owned_process(conn, owner, false)?;
      }
      diesel::delete(processes::table.find(&key)).execute(conn)?;
      Ok(())
    })
    .await
  }

  /// Read network owners independently of the displayed page and its filters.
  pub async fn read_network_owners(
    targets: &[(String, String)],
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    if targets.is_empty() {
      return Ok(Vec::new());
    }
    let targets = targets.to_vec();
    let pool = pool.clone();
    ntex::rt::spawn_blocking(move || {
      let mut conn = crate::utils::store::get_pool_conn(&pool)?;
      let mut query = processes::table
        .filter(false.into_sql::<diesel::sql_types::Bool>())
        .into_boxed();
      for (node, target) in &targets {
        query = query.or_filter(
          processes::node_name
            .eq(node)
            .and(processes::key.eq(target).or(processes::name.eq(target))),
        );
      }
      query
        .load::<ProcessDb>(&mut conn)
        .map_err(Self::map_err)?
        .into_iter()
        .map(Process::try_from)
        .collect()
    })
    .await
    .map_err(|err| {
      IoError::interrupted("Read process network owners", &err.to_string())
    })?
  }

  pub async fn read_by_kind_key(
    kind_key: &str,
    filter: Option<GenericFilter>,
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    let filter = filter
      .unwrap_or_default()
      .r#where("kind_key", GenericClause::Eq(kind_key.to_owned()));
    ProcessDb::transform_read_by(&filter, pool).await
  }
}

impl ProcessDb {
  pub async fn list_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    let filter = GenericFilter::new().r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "Config": {
          "Labels": {
            "io.nanocl.n": name
          }
        }
      })),
    );
    ProcessDb::transform_read_by(&filter, pool).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    models::{
      DistributedMutexDb, DistributedMutexPartial, NamespaceDb, NodeDb,
      OperationDb, SystemState, VmDb, VmObjCreateIn,
    },
    objects::generic::ObjCreate,
    utils::tests::gen_test_system,
    vars,
  };
  use nanocl_stubs::{
    process::ProcessKind, system::ObjPsStatusKind, vm_spec::VmSpecPartial,
  };

  fn no_routes(_: &mut ntex::web::ServiceConfig<SystemState>) {}

  #[ntex::test]
  async fn owned_vm_process_uses_process_kind_and_preserves_owner_fences() {
    let system = gen_test_system(no_routes, vars::VERSION).await;
    let state = &system.state;
    let pool = &state.inner.pool;
    NodeDb::register(state).await.unwrap();
    let namespace = format!("process-owner-{}", uuid::Uuid::new_v4().simple());
    NamespaceDb::create_from(NamespaceDb::new(&namespace), pool)
      .await
      .unwrap();
    let vm = VmDb::fn_create_obj(
      &VmObjCreateIn {
        namespace: namespace.clone(),
        version: vars::VERSION.into(),
        spec: VmSpecPartial {
          name: "guest".into(),
          image: "/process-owner-test.qcow2".into(),
          ..Default::default()
        },
      },
      state,
    )
    .await
    .unwrap();
    let incarnation = VmDb::read_by_pk(&vm.spec.vm_key, pool)
      .await
      .unwrap()
      .0
      .incarnation_key;
    let mutex = DistributedMutexDb::create_from(
      DistributedMutexPartial {
        action: "start".into(),
        resource_kind: "Vm".into(),
        resource_key: vm.spec.vm_key.clone(),
        node_key: state.inner.config.hostname.clone(),
        acquired_by: "process-owner-test".into(),
      },
      pool,
    )
    .await
    .unwrap();
    let owner = OperationOwner {
      key: mutex.key,
      incarnation_key: incarnation,
      resource_kind: "Vm".into(),
      resource_key: vm.spec.vm_key.clone(),
      node_name: mutex.node_key,
      owner_boot_id: "process-owner-test".into(),
      lock_device: 1,
      lock_inode: 2,
    };
    let operation =
      OperationDb::new(&owner, "Vm", &vm.spec.vm_key, vm.spec.key, "start");
    VmDb::accept_operation(&vm, None, operation.clone(), (), pool)
      .await
      .unwrap();
    let process = ProcessPartial {
      key: uuid::Uuid::new_v4().to_string(),
      name: format!("{}.v", vm.spec.vm_key),
      kind: ProcessKind::Vm,
      data: serde_json::json!({}),
      node_name: owner.node_name.clone(),
      kind_key: vm.spec.vm_key.clone(),
      created_at: None,
    };
    let stored =
      ProcessDb::create_owned(&process, Some(owner.clone()), (), pool)
        .await
        .unwrap();
    assert_eq!(stored.kind, "vm");
    assert_eq!(owner.resource_kind, "Vm");
    assert_eq!(stored.kind_key, operation.resource_key);
    ProcessDb::delete_owned(&process.key, Some(owner.clone()), (), pool)
      .await
      .unwrap();
    assert!(ProcessDb::read_by_pk(&process.key, pool).await.is_err());

    let mut wrong_kind = process.clone();
    wrong_kind.kind = ProcessKind::Cargo;
    let mut wrong_resource = process.clone();
    wrong_resource.kind_key.push_str("-other");
    let mut wrong_node = process.clone();
    wrong_node.node_name.push_str("-other");
    for invalid in [&wrong_kind, &wrong_resource, &wrong_node] {
      let error =
        ProcessDb::create_owned(invalid, Some(owner.clone()), (), pool)
          .await
          .err()
          .unwrap();
      assert!(error.to_string().contains("does not belong"), "{error}");
      assert!(ProcessDb::read_by_pk(&process.key, pool).await.is_err());
    }
    // An existing process belonging to another resource/kind must not be deleted.
    for foreign in [&wrong_kind, &wrong_resource] {
      ProcessDb::create_from(foreign, pool).await.unwrap();
      let error =
        ProcessDb::delete_owned(&foreign.key, Some(owner.clone()), (), pool)
          .await
          .unwrap_err();
      assert!(error.to_string().contains("does not belong"), "{error}");
      assert!(ProcessDb::read_by_pk(&foreign.key, pool).await.is_ok());
      ProcessDb::del_by_pk(&foreign.key, pool).await.unwrap();
    }
    ProcessDb::create_owned(&process, Some(owner.clone()), (), pool)
      .await
      .unwrap();
    assert!(
      VmDb::finish_operation(
        &operation,
        ObjPsStatusKind::Stop,
        "interrupted",
        serde_json::Value::Null,
        (),
        pool
      )
      .await
      .unwrap()
    );
    assert!(
      ProcessDb::delete_owned(&process.key, Some(owner), (), pool)
        .await
        .is_err()
    );
    assert!(ProcessDb::read_by_pk(&process.key, pool).await.is_ok());
    ProcessDb::del_by_pk(&process.key, pool).await.unwrap();
    VmDb::clear_by_pk(&vm.spec.vm_key, pool).await.unwrap();
    NamespaceDb::del_by_pk(&namespace, pool).await.unwrap();
    state.wait_event_loop().await;
  }
}
