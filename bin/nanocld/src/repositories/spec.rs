use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  cargo_spec::{CargoSpecPartial, CargoSpec},
  vm_spec::{VmSpec, VmSpecPartial},
};

use crate::{
  gen_where4string,
  models::{Pool, SpecDb},
  schema::specs,
};

use super::generic::*;

impl RepositoryBase for SpecDb {}

impl RepositoryCreate for SpecDb {}

impl RepositoryRead for SpecDb {
  type Output = SpecDb;
  type Query = specs::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = specs::table.into_boxed();
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, specs::kind_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, specs::version, value);
    }
    if is_multiple {
      query = query.order(specs::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

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
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = diesel::delete(specs::table).into_boxed();
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, specs::kind_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, specs::version, value);
    }
    query
  }
}

impl SpecDb {
  pub(crate) async fn del_by_kind_key(key: &str, pool: &Pool) -> IoResult<()> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(key.to_owned()));
    Self::del_by(&filter, pool).await
  }

  pub(crate) async fn get_version(
    name: &str,
    version: &str,
    pool: &Pool,
  ) -> IoResult<SpecDb> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(name.to_owned()))
      .r#where("version", GenericClause::Eq(version.to_owned()));
    let item = SpecDb::read_one(&filter, pool).await?;
    Ok(item)
  }

  pub(crate) async fn read_by_kind_key(
    key: &str,
    pool: &Pool,
  ) -> IoResult<Vec<SpecDb>> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(key.to_owned()));
    let items = SpecDb::read(&filter, pool).await?;
    Ok(items)
  }

  pub fn try_from_cargo_partial(
    key: &str,
    version: &str,
    item: &CargoSpecPartial,
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
    let p = serde_json::from_value::<CargoSpecPartial>(self.data.clone())?;
    let spec = CargoSpec {
      key: self.key,
      cargo_key: self.kind_key.clone(),
      version: self.version.clone(),
      created_at: self.created_at,
      name: p.name,
      metadata: self.metadata.clone(),
      init_container: p.init_container,
      secrets: p.secrets,
      container: p.container,
      replication: p.replication,
    };
    Ok(spec)
  }

  pub fn try_to_vm_spec(&self) -> IoResult<VmSpec> {
    let p = serde_json::from_value::<VmSpecPartial>(self.data.clone())?;
    let spec = VmSpec {
      key: self.key,
      vm_key: self.kind_key.clone(),
      version: self.version.clone(),
      created_at: self.created_at,
      name: p.name,
      metadata: self.metadata.clone(),
      hostname: p.hostname,
      password: p.password,
      disk: p.disk,
      host_config: p.host_config.unwrap_or_default(),
      ssh_key: p.ssh_key,
      user: p.user,
      mac_address: p.mac_address,
      labels: p.labels,
    };
    Ok(spec)
  }
}
