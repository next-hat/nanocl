use std::collections::HashMap;
use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoResult, IoError, FromIo};

use nanocl_stubs::generic::{GenericFilter, GenericClause};
use nanocl_stubs::vm::Vm;
use nanocl_stubs::vm_spec::{VmSpec, VmSpecPartial};

use crate::utils;
use crate::schema::vms;

use super::{Pool, Repository, FromSpec, WithSpec, VmSpecDb, NamespaceDb};

/// This structure represent the vm in the database.
/// A vm is a virtual machine that is running on the server.
/// The vm is linked to a namespace.
/// We use the `spec_key` to link to the vm spec.
/// The `key` is used to identify the vm and is generated as follow: `namespace_name-vm_name`.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vms)]
#[diesel(belongs_to(NamespaceDb, foreign_key = namespace_name))]
pub struct VmDb {
  /// The key of the vm
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The name of the vm
  pub name: String,
  /// The spec key reference
  pub spec_key: uuid::Uuid,
  /// The namespace name reference
  pub namespace_name: String,
}

impl WithSpec for VmDb {
  type Type = Vm;
  type Relation = VmSpec;

  fn with_spec(self, r: &Self::Relation) -> Self::Type {
    Self::Type {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.clone(),
    }
  }
}

/// This structure is used to update a vm in the database.
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = vms)]
pub struct VmUpdateDb {
  /// The key of the vm
  pub key: Option<String>,
  /// The namespace name reference
  pub namespace_name: Option<String>,
  /// The name of the vm
  pub name: Option<String>,
  /// The spec key reference
  pub spec_key: Option<uuid::Uuid>,
}

impl Repository for VmDb {
  type Table = vms::table;
  type Item = Vm;
  type UpdateItem = VmUpdateDb;

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

impl VmDb {
  pub(crate) async fn create_from_spec(
    nsp: &str,
    item: &VmSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Vm> {
    let nsp = nsp.to_owned();
    if item.name.contains('.') {
      return Err(IoError::invalid_data(
        "VmSpecPartial",
        "Name cannot contain a dot.",
      ));
    }
    let key = utils::key::gen_key(&nsp, &item.name);
    let new_spec = VmSpecDb::try_from_spec_partial(&key, version, item)?;
    let spec = VmSpecDb::create(new_spec, pool).await??.to_spec(item);
    let new_item = VmDb {
      key,
      name: item.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: nsp,
      spec_key: spec.key,
    };
    let item = VmDb::create(new_item, pool).await??;
    let vm = item.with_spec(&spec);
    Ok(vm)
  }

  pub(crate) async fn update_from_spec(
    key: &str,
    item: &VmSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Vm> {
    let vmdb = VmDb::find_by_pk(key, pool).await??;
    let new_spec = VmSpecDb::try_from_spec_partial(&vmdb.key, version, item)?;
    let spec = VmSpecDb::create(new_spec, pool).await??.to_spec(item);
    let new_item = VmUpdateDb {
      name: Some(item.name.clone()),
      spec_key: Some(spec.key),
      ..Default::default()
    };
    VmDb::update_by_pk(key, new_item, pool).await??;
    let vm = vmdb.with_spec(&spec);
    Ok(vm)
  }

  pub(crate) async fn inspect_by_pk(key: &str, pool: &Pool) -> IoResult<Vm> {
    use crate::schema::vm_specs;
    let key = key.to_owned();
    let pool = Arc::clone(pool);
    let item: (VmDb, VmSpecDb) = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = vms::table
        .inner_join(vm_specs::table)
        .filter(vms::key.eq(key))
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| "Vm"))?;
      Ok::<_, IoError>(item)
    })
    .await?;
    let spec = item.1.try_to_spec()?;
    let item = item.0.with_spec(&spec);
    Ok(item)
  }

  pub(crate) async fn find_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Vm>> {
    let mut r#where = HashMap::new();
    r#where.insert(
      "NamespaceName".to_owned(),
      GenericClause::Eq(name.to_owned()),
    );
    let filter = GenericFilter {
      r#where: Some(r#where),
    };
    VmDb::find(&filter, pool).await?
  }
}
