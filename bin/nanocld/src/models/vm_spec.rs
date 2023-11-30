use std::collections::HashMap;

use nanocl_error::io::IoResult;
use nanocl_stubs::generic::{GenericFilter, GenericClause};
use nanocl_stubs::vm::Vm;
use nanocl_stubs::vm_spec::{VmSpec, VmSpecPartial};
use tokio::task::JoinHandle;

use crate::schema::vm_specs;

use super::{Repository, Pool};
use super::vm::VmDb;
use super::generic::FromSpec;

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
    unimplemented!()
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
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
}

impl VmSpecDb {
  pub(crate) async fn find_by_vm(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<VmSpec>> {
    let mut r#where = HashMap::new();
    r#where.insert(
      "NamespaceName".to_owned(),
      GenericClause::Eq(name.to_owned()),
    );
    let filter = GenericFilter {
      r#where: Some(r#where),
    };
    VmSpecDb::find(&filter, pool).await?
  }
}
