use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::{GenericFilter, GenericClause};

use crate::{
  gen_multiple, gen_where4string,
  models::{Pool, VmImageDb, VmImageUpdateDb},
  schema::vm_images,
};

use super::generic::*;

impl RepositoryBase for VmImageDb {}

impl RepositoryCreate for VmImageDb {}

impl RepositoryUpdate for VmImageDb {
  type UpdateItem = VmImageUpdateDb;
}

impl RepositoryDelByPk for VmImageDb {}

impl RepositoryReadBy for VmImageDb {
  type Output = VmImageDb;

  fn get_pk() -> &'static str {
    "name"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::pg::PgConnection,
    Self::Output,
  > {
    let condition = filter.r#where.clone().unwrap_or_default();
    let r#where = condition.r#where;
    let mut query = vm_images::table.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vm_images::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, vm_images::kind, value);
    }
    if let Some(value) = r#where.get("parent") {
      gen_where4string!(query, vm_images::parent, value);
    }
    if let Some(value) = r#where.get("format") {
      gen_where4string!(query, vm_images::format, value);
    }
    if let Some(value) = r#where.get("path") {
      gen_where4string!(query, vm_images::path, value);
    }
    if is_multiple {
      gen_multiple!(query, vm_images::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for VmImageDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let condition = filter.r#where.clone().unwrap_or_default();
    let r#where = condition.r#where;
    let mut query = vm_images::table.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vm_images::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, vm_images::kind, value);
    }
    if let Some(value) = r#where.get("parent") {
      gen_where4string!(query, vm_images::parent, value);
    }
    if let Some(value) = r#where.get("format") {
      gen_where4string!(query, vm_images::format, value);
    }
    if let Some(value) = r#where.get("path") {
      gen_where4string!(query, vm_images::path, value);
    }
    query.count()
  }
}

impl VmImageDb {
  pub async fn read_by_parent(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<VmImageDb>> {
    let filter = GenericFilter::new()
      .r#where("parent", GenericClause::Eq(name.to_owned()));
    VmImageDb::read_by(&filter, pool).await
  }
}
