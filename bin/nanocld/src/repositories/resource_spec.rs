use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use crate::{
  gen_where4json, gen_where4string, models::ResourceSpecDb,
  schema::resource_specs,
};

use super::generic::*;

impl RepositoryBase for ResourceSpecDb {}

impl RepositoryCreate for ResourceSpecDb {}

impl RepositoryRead for ResourceSpecDb {
  type Output = ResourceSpecDb;
  type Query = resource_specs::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_specs::dsl::resource_specs.into_boxed();
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_specs::dsl::version, value);
    }
    if let Some(value) = r#where.get("resource_key") {
      gen_where4string!(query, resource_specs::dsl::resource_key, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, resource_specs::dsl::data, value);
    }
    if let Some(value) = r#where.get("metadata") {
      gen_where4json!(query, resource_specs::dsl::metadata, value);
    }
    if is_multiple {
      query = query.order(resource_specs::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl RepositoryDelBy for ResourceSpecDb {
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
    let mut query = diesel::delete(resource_specs::table).into_boxed();
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_specs::dsl::version, value);
    }
    if let Some(value) = r#where.get("resource_key") {
      gen_where4string!(query, resource_specs::dsl::resource_key, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, resource_specs::dsl::data, value);
    }
    if let Some(value) = r#where.get("metadata") {
      gen_where4json!(query, resource_specs::dsl::metadata, value);
    }
    query
  }
}
