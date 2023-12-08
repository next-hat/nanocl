use std::sync::Arc;
use std::collections::HashMap;

use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoError, IoResult, FromIo};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  vm_spec::{VmSpec, VmSpecPartial},
};

use crate::{utils, gen_where4string, schema::vm_specs};

use super::{Pool, Repository, VmDb, FromSpec};

/// This structure represent the vm spec in the database.
/// A vm spec represent the specification of a virtual machine.
/// It is stored as a json object in the database.
/// We use the `vm_key` to link to the vm.
/// And the version is used to know which version of the spec is used
/// to ensure consistency between updates.
#[derive(Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vm_specs)]
#[diesel(belongs_to(VmDb, foreign_key = vm_key))]
pub struct VmSpecDb {
  /// The key of the vm spec
  pub key: uuid::Uuid,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The vm key reference
  pub vm_key: String,
  /// The version of the vm spec
  pub version: String,
  /// The spec of the vm
  pub data: serde_json::Value,
  /// The metadata (user defined)
  pub metadata: Option<serde_json::Value>,
}

impl Repository for VmSpecDb {
  type Table = vm_specs::table;
  type Item = VmSpec;
  type UpdateItem = VmSpecDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::debug!("VmSpecDb::find_one filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = vm_specs::dsl::vm_specs.into_boxed();
    if let Some(value) = r#where.get("vm_key") {
      gen_where4string!(query, vm_specs::dsl::vm_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, vm_specs::dsl::version, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(Self::map_err_context)?
        .try_to_spec()?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::debug!("VmSpecDb::find filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = vm_specs::dsl::vm_specs.into_boxed();
    if let Some(value) = r#where.get("vm_key") {
      gen_where4string!(query, vm_specs::dsl::vm_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, vm_specs::dsl::version, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err_context)?
        .into_iter()
        .map(|i| i.try_to_spec())
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
  }
}

impl FromSpec for VmSpecDb {
  type Spec = VmSpec;
  type SpecPartial = VmSpecPartial;

  fn try_from_spec_partial(
    id: &str,
    version: &str,
    p: &Self::SpecPartial,
  ) -> IoResult<Self> {
    let data = VmSpecDb::try_to_data(p)?;
    Ok(VmSpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      vm_key: id.to_owned(),
      version: version.to_owned(),
      data,
      metadata: p.metadata.clone(),
    })
  }

  fn get_data(&self) -> &serde_json::Value {
    &self.data
  }

  fn to_spec(&self, p: &Self::SpecPartial) -> Self::Spec {
    Self::Spec {
      key: self.key,
      created_at: self.created_at,
      name: p.name.clone(),
      version: self.version.clone(),
      vm_key: self.vm_key.clone(),
      disk: p.disk.clone(),
      host_config: p.host_config.clone().unwrap_or_default(),
      hostname: p.hostname.clone(),
      user: p.user.clone(),
      labels: p.labels.clone(),
      mac_address: p.mac_address.clone(),
      password: p.password.clone(),
      ssh_key: p.ssh_key.clone(),
      metadata: p.metadata.clone(),
    }
  }

  fn try_to_spec(&self) -> IoResult<Self::Spec>
  where
    Self::SpecPartial: serde::de::DeserializeOwned,
    Self::Spec: std::marker::Sized,
  {
    let p =
      serde_json::from_value::<Self::SpecPartial>(self.get_data().clone())
        .map_err(|err| err.map_err_context(|| "Spec"))?;
    let mut spec = self.to_spec(&p);
    spec.metadata = self.metadata.clone();
    Ok(spec)
  }
}

impl VmSpecDb {
  pub(crate) async fn find_by_vm(
    vm_pk: &str,
    pool: &Pool,
  ) -> IoResult<Vec<VmSpec>> {
    let mut r#where = HashMap::new();
    r#where.insert("VmKey".to_owned(), GenericClause::Eq(vm_pk.to_owned()));
    let filter = GenericFilter {
      r#where: Some(r#where),
    };
    VmSpecDb::find(&filter, pool).await?
  }
}
