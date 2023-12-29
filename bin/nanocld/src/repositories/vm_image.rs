use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::{GenericFilter, GenericClause};

use crate::{
  gen_where4string,
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

impl RepositoryRead for VmImageDb {
  type Output = VmImageDb;
  type Query = vm_images::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = vm_images::dsl::vm_images.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vm_images::dsl::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, vm_images::dsl::kind, value);
    }
    if let Some(value) = r#where.get("parent") {
      gen_where4string!(query, vm_images::dsl::parent, value);
    }
    if let Some(value) = r#where.get("format") {
      gen_where4string!(query, vm_images::dsl::format, value);
    }
    if let Some(value) = r#where.get("path") {
      gen_where4string!(query, vm_images::dsl::path, value);
    }
    if is_multiple {
      query = query.order(vm_images::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl VmImageDb {
  pub(crate) async fn find_by_parent(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<VmImageDb>> {
    let filter = GenericFilter::new()
      .r#where("parent", GenericClause::Eq(name.to_owned()));
    VmImageDb::read(&filter, pool).await
  }
}
