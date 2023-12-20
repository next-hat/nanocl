use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::io::IoResult;
use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  vm_spec::VmSpec,
};

use crate::{
  gen_where4string,
  models::{VmSpecDb, Pool, FromSpec},
  schema::vm_specs,
};

use super::generic::*;

impl RepositoryBase for VmSpecDb {}

impl RepositoryCreate for VmSpecDb {}

impl RepositoryDelByPk for VmSpecDb {}

impl RepositoryRead for VmSpecDb {
  type Output = VmSpecDb;
  type Query = vm_specs::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = vm_specs::dsl::vm_specs.into_boxed();
    if let Some(value) = r#where.get("vm_key") {
      gen_where4string!(query, vm_specs::dsl::vm_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, vm_specs::dsl::version, value);
    }
    if is_multiple {
      query = query.order(vm_specs::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl RepositoryDelBy for VmSpecDb {
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
    let mut query = diesel::delete(vm_specs::table).into_boxed();
    if let Some(value) = r#where.get("vm_key") {
      gen_where4string!(query, vm_specs::vm_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, vm_specs::version, value);
    }
    query
  }
}

impl VmSpecDb {
  pub(crate) async fn find_by_vm(
    vm_pk: &str,
    pool: &Pool,
  ) -> IoResult<Vec<VmSpec>> {
    let mut r#where = HashMap::new();
    r#where.insert("vm_key".to_owned(), GenericClause::Eq(vm_pk.to_owned()));
    let filter = GenericFilter {
      r#where: Some(r#where),
      ..Default::default()
    };
    let items = VmSpecDb::read(&filter, pool)
      .await??
      .into_iter()
      .map(|i| i.try_to_spec())
      .collect::<IoResult<Vec<_>>>()?;
    Ok(items)
  }
}
