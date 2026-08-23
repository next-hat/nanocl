use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::{
  cargo_spec::{CargoSpec, CargoSpecRevision},
  generic::{GenericClause, GenericFilter},
  vm_spec::{VmSpec, VmSpecPartial},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, Pool, SpecDb},
  schema::specs,
};

use super::generic::*;

impl RepositoryBase for SpecDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Uuid, "specs.key")),
      ("created_at", (ColumnType::Timestamptz, "specs.created_at")),
      ("kind_name", (ColumnType::Text, "specs.kind_name")),
      ("kind_key", (ColumnType::Text, "specs.kind_key")),
      ("version", (ColumnType::Text, "specs.version")),
      ("data", (ColumnType::Json, "specs.data")),
      ("metadata", (ColumnType::Json, "specs.metadata")),
    ])
  }
}

impl RepositoryCreate for SpecDb {}

impl RepositoryDelBy for SpecDb {
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
    let mut query = diesel::delete(specs::table).into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns)
  }
}

impl RepositoryReadBy for SpecDb {
  type Output = SpecDb;

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
    let mut query = specs::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(specs::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl SpecDb {
  pub async fn del_by_kind_key(key: &str, pool: &Pool) -> IoResult<()> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(key.to_owned()));
    SpecDb::del_by(&filter, pool).await
  }

  pub async fn get_version(
    name: &str,
    version: &str,
    pool: &Pool,
  ) -> IoResult<SpecDb> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(name.to_owned()))
      .r#where("version", GenericClause::Eq(version.to_owned()));
    SpecDb::read_one_by(&filter, pool).await
  }

  pub async fn read_by_kind_key(
    key: &str,
    pool: &Pool,
  ) -> IoResult<Vec<SpecDb>> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(key.to_owned()));
    SpecDb::read_by(&filter, pool).await
  }

  pub fn try_from_cargo_spec(
    key: &str,
    version: &str,
    item: &CargoSpec,
  ) -> IoResult<Self> {
    Ok(Self {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: "Cargo".to_owned(),
      kind_key: key.to_owned(),
      version: version.to_owned(),
      data: serde_json::to_value(item)?,
      metadata: item.metadata.clone(),
    })
  }

  pub fn try_from_vm_partial(
    key: &str,
    version: &str,
    item: &VmSpecPartial,
  ) -> IoResult<Self> {
    Ok(Self {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: "Vm".to_owned(),
      kind_key: key.to_owned(),
      version: version.to_owned(),
      data: serde_json::to_value(item)?,
      metadata: item.metadata.clone(),
    })
  }

  pub fn try_to_cargo_spec(&self) -> IoResult<CargoSpec> {
    Ok(serde_json::from_value::<CargoSpec>(self.data.clone())?)
  }

  pub fn try_to_cargo_spec_revision(&self) -> IoResult<CargoSpecRevision> {
    let spec = self.try_to_cargo_spec()?;
    let revision = CargoSpecRevision {
      key: self.key,
      cargo_key: self.kind_key.clone(),
      version: self.version.clone(),
      created_at: self.created_at,
      name: spec.name,
      metadata: spec.metadata,
      replicas: spec.replicas,
      network_mode: spec.network_mode,
      port_bindings: spec.port_bindings,
      hostname: spec.hostname,
      dns: spec.dns,
      secrets: spec.secrets,
      placement: spec.placement,
      resource_requirement: spec.resource_requirement,
      init_containers: spec.init_containers,
      containers: spec.containers,
    };
    Ok(revision)
  }

  pub fn try_to_vm_spec(&self) -> IoResult<VmSpec> {
    let vm_spec_partial =
      serde_json::from_value::<VmSpecPartial>(self.data.clone())?;
    let spec = VmSpec {
      key: self.key,
      vm_key: self.kind_key.clone(),
      version: self.version.clone(),
      created_at: self.created_at,
      name: vm_spec_partial.name,
      metadata: self.metadata.clone(),
      hostname: vm_spec_partial.hostname,
      password: vm_spec_partial.password,
      image: vm_spec_partial.image,
      host_config: vm_spec_partial.host_config.unwrap_or_default(),
      ssh_key: vm_spec_partial.ssh_key,
      init_container: vm_spec_partial.init_container,
      user: vm_spec_partial.user,
      mac_address: vm_spec_partial.mac_address,
      labels: vm_spec_partial.labels,
    };
    Ok(spec)
  }
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::{
    cargo_spec::{
      CargoNetworkMode, CargoPlacementSpec, CargoPlacementStrategy,
      CargoResourceRequirementSpec, Config, ContainerSpec, PortBinding,
      PortMap,
    },
    generic::ImagePullPolicy,
  };

  use super::*;

  fn container(name: &str, image: &str) -> ContainerSpec {
    ContainerSpec {
      name: name.to_owned(),
      essential: true,
      secrets: vec![format!("{name}-secret")],
      image_pull_secret: Some("registry-secret".to_owned()),
      image_pull_policy: ImagePullPolicy::Always,
      container_config: Config {
        image: Some(image.to_owned()),
        env: Some(vec![format!("ROLE={name}")]),
        ..Default::default()
      },
    }
  }

  fn complete_spec() -> CargoSpec {
    CargoSpec {
      name: "persisted-cargo".to_owned(),
      metadata: Some(serde_json::json!({"owner": "platform"})),
      replicas: 3,
      network_mode: Some(CargoNetworkMode::new("private").unwrap()),
      port_bindings: Some(PortMap::from([(
        "8080/tcp".to_owned(),
        Some(vec![PortBinding {
          host_ip: Some("127.0.0.1".to_owned()),
          host_port: Some("18080".to_owned()),
        }]),
      )])),
      hostname: Some("persisted-host".to_owned()),
      dns: Some(vec!["172.18.0.1".to_owned()]),
      secrets: vec!["shared-secret".to_owned()],
      placement: Some(CargoPlacementSpec {
        strategy: Some(CargoPlacementStrategy::Distinct),
        ..Default::default()
      }),
      resource_requirement: Some(CargoResourceRequirementSpec {
        cpu_cores: Some(2),
        cpu_utilization_cap: Some(0.75),
        memory_bytes: Some(512 * 1024 * 1024),
        memory_utilization_cap: Some(0.8),
        cpu_weight: Some(0.6),
        memory_weight: Some(0.4),
        storage_bytes: Some(1024 * 1024 * 1024),
      }),
      init_containers: vec![container("migrate", "example/migrate:1")],
      containers: vec![
        container("api", "example/api:1"),
        container("worker", "example/worker:1"),
      ],
    }
  }

  #[test]
  fn cargo_desired_spec_json_round_trips_every_final_field() {
    let desired = complete_spec();
    let stored =
      SpecDb::try_from_cargo_spec("team.persisted-cargo", "v0.18.0", &desired)
        .unwrap();

    assert_eq!(stored.data, serde_json::to_value(&desired).unwrap());
    assert_eq!(stored.metadata, desired.metadata);
    assert_eq!(stored.try_to_cargo_spec().unwrap(), desired);
  }

  #[test]
  fn cargo_revision_reconstruction_preserves_declaration_and_metadata() {
    let desired = complete_spec();
    let stored =
      SpecDb::try_from_cargo_spec("team.persisted-cargo", "v0.18.0", &desired)
        .unwrap();

    let revision = stored.try_to_cargo_spec_revision().unwrap();

    assert_eq!(revision.key, stored.key);
    assert_eq!(revision.cargo_key, stored.kind_key);
    assert_eq!(revision.version, stored.version);
    assert_eq!(revision.created_at, stored.created_at);
    assert_eq!(revision.name, desired.name);
    assert_eq!(revision.metadata, desired.metadata);
    assert_eq!(revision.replicas, desired.replicas);
    assert_eq!(revision.network_mode, desired.network_mode);
    assert_eq!(revision.port_bindings, desired.port_bindings);
    assert_eq!(revision.hostname, desired.hostname);
    assert_eq!(revision.dns, desired.dns);
    assert_eq!(revision.secrets, desired.secrets);
    assert_eq!(revision.placement, desired.placement);
    assert_eq!(revision.resource_requirement, desired.resource_requirement);
    assert_eq!(revision.init_containers, desired.init_containers);
    assert_eq!(revision.containers, desired.containers);
  }

  #[test]
  fn revert_extraction_restores_the_complete_desired_declaration() {
    let desired = complete_spec();
    let mut history =
      SpecDb::try_from_cargo_spec("team.persisted-cargo", "v0.18.0", &desired)
        .unwrap();
    // The generic metadata column is only a query mirror. Revert restores the
    // authoritative complete declaration from specs.data.
    history.metadata = Some(serde_json::json!({"stale-mirror": true}));

    let restored = history.try_to_cargo_spec().unwrap();

    assert_eq!(restored, desired);
  }
}
