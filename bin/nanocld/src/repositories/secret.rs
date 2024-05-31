use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use nanocl_stubs::secret::Secret;

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, SecretDb, SecretUpdateDb},
  schema::secrets,
};

use super::generic::*;

impl RepositoryBase for SecretDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "secrets.key")),
      ("kind", (ColumnType::Text, "secrets.kind")),
      (
        "created_at",
        (ColumnType::Timestamptz, "secrets.created_at"),
      ),
      (
        "updated_at",
        (ColumnType::Timestamptz, "secrets.updated_at"),
      ),
      ("data", (ColumnType::Json, "secrets.data")),
      ("metadata", (ColumnType::Json, "secrets.metadata")),
    ])
  }
}

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
    let mut query = secrets::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(secrets::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for SecretDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = secrets::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
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
