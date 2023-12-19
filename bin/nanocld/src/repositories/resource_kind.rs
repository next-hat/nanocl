use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::{GenericFilter, GenericClause};

use crate::{
  gen_where4string,
  models::{Pool, ResourceKindDb, ResourceKindVersionDb},
  schema::{resource_kinds, resource_kind_versions},
};

use super::generic::*;

impl RepositoryBase for ResourceKindDb {}

impl RepositoryCreate for ResourceKindDb {}

impl RepositoryDelByPk for ResourceKindDb {}

impl RepositoryRead for ResourceKindDb {
  type Output = ResourceKindDb;
  type Query = resource_kinds::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::dsl::resource_kinds.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, resource_kinds::dsl::name, value);
    }
    if is_multiple {
      query = query.order(resource_kinds::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl RepositoryBase for ResourceKindVersionDb {}

impl RepositoryCreate for ResourceKindVersionDb {}

impl RepositoryDelByPk for ResourceKindVersionDb {}

impl RepositoryRead for ResourceKindVersionDb {
  type Output = ResourceKindVersionDb;
  type Query = resource_kind_versions::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query =
      resource_kind_versions::dsl::resource_kind_versions.into_boxed();
    if let Some(value) = r#where.get("resource_kind_name") {
      gen_where4string!(
        query,
        resource_kind_versions::dsl::resource_kind_name,
        value
      );
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_kind_versions::dsl::version, value);
    }
    if is_multiple {
      query = query.order(resource_kind_versions::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl RepositoryDelBy for ResourceKindVersionDb {
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
    let mut query = diesel::delete(resource_kind_versions::table).into_boxed();
    if let Some(value) = r#where.get("resource_kind_name") {
      gen_where4string!(
        query,
        resource_kind_versions::dsl::resource_kind_name,
        value
      );
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_kind_versions::dsl::version, value);
    }
    query
  }
}

impl ResourceKindVersionDb {
  pub(crate) async fn get_version(
    name: &str,
    version: &str,
    pool: &Pool,
  ) -> IoResult<ResourceKindVersionDb> {
    let filter = GenericFilter::new()
      .r#where("resource_kind_name", GenericClause::Eq(name.to_owned()))
      .r#where("version", GenericClause::Eq(version.to_owned()));
    ResourceKindVersionDb::read_one(&filter, pool).await?
  }
}
