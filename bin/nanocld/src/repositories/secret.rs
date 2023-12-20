use diesel::prelude::*;
use nanocl_stubs::generic::GenericFilter;

use nanocl_stubs::secret::Secret;

use crate::{
  gen_where4string,
  models::{SecretDb, SecretUpdateDb},
  schema::secrets,
};

use super::generic::*;

impl RepositoryBase for SecretDb {}

impl RepositoryCreate for SecretDb {}

impl RepositoryDelByPk for SecretDb {}

impl RepositoryUpdate for SecretDb {
  type UpdateItem = SecretUpdateDb;
}

impl RepositoryRead for SecretDb {
  type Output = Secret;
  type Query = secrets::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = secrets::dsl::secrets.into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, secrets::dsl::key, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, secrets::dsl::kind, value);
    }
    if is_multiple {
      query = query.order(secrets::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}
