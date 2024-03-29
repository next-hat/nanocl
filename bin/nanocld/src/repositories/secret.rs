use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use nanocl_stubs::secret::Secret;

use crate::{
  gen_multiple, gen_where4string,
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

impl RepositoryReadBy for SecretDb {
  type Output = SecretDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::pg::PgConnection,
    Self::Output,
  > {
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = secrets::table.into_boxed();
    if let Some(key) = r#where.get("key") {
      gen_where4string!(query, secrets::key, key);
    }
    if let Some(kind) = r#where.get("kind") {
      gen_where4string!(query, secrets::kind, kind);
    }
    if is_multiple {
      gen_multiple!(query, secrets::created_at, filter);
    }
    query
  }
}

impl RepositoryReadByTransform for SecretDb {
  type NewOutput = Secret;

  fn transform(
    input: Self::Output,
  ) -> nanocl_error::io::IoResult<Self::NewOutput> {
    input.try_into()
  }
}
