use std::collections::HashMap;

use diesel::prelude::*;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::generic::GenericFilter;

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, DistributedMutexDb, Pool},
  schema::distributed_mutexes,
  utils,
};

use super::generic::*;

impl RepositoryBase for DistributedMutexDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Uuid, "distributed_mutexes.key")),
      (
        "resource_kind",
        (ColumnType::Text, "distributed_mutexes.resource_kind"),
      ),
      (
        "resource_key",
        (ColumnType::Text, "distributed_mutexes.resource_key"),
      ),
      (
        "node_key",
        (ColumnType::Text, "distributed_mutexes.node_key"),
      ),
      (
        "acquired_by",
        (ColumnType::Text, "distributed_mutexes.acquired_by"),
      ),
      (
        "acquired_at",
        (ColumnType::Timestamptz, "distributed_mutexes.acquired_at"),
      ),
      (
        "expires_at",
        (ColumnType::Timestamptz, "distributed_mutexes.expires_at"),
      ),
    ])
  }
}

impl RepositoryCreate for DistributedMutexDb {}

impl RepositoryReadBy for DistributedMutexDb {
  type Output = DistributedMutexDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &nanocl_stubs::generic::GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::PgConnection,
    Self::Output,
  >
  where
    Self::Output: Sized,
  {
    let mut query = distributed_mutexes::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(distributed_mutexes::acquired_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for DistributedMutexDb {
  fn gen_count_query(
    filter: &nanocl_stubs::generic::GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = distributed_mutexes::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryDelByPk for DistributedMutexDb {}

impl RepositoryDelBy for DistributedMutexDb {
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
    let mut query = diesel::delete(distributed_mutexes::table).into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns)
  }
}

impl DistributedMutexDb {
  pub async fn get_lock(
    resource_kind: &str,
    resource_key: &str,
    pool: &Pool,
  ) -> IoResult<DistributedMutexDb> {
    let pool = pool.clone();
    let resource_kind = resource_kind.to_owned();
    let resource_key = resource_key.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let query = distributed_mutexes::table
        .filter(distributed_mutexes::resource_kind.eq(resource_kind))
        .filter(distributed_mutexes::resource_key.eq(resource_key));
      let item = query.get_result(&mut conn).map_err(Self::map_err)?;
      Ok(item)
    })
    .await
    .map_err(|err| {
      IoError::interrupted(
        "Interrupted while getting lock",
        err.to_string().as_str(),
      )
    })?
  }

  pub async fn del_lock(
    resource_kind: &str,
    resource_key: &str,
    pool: &Pool,
  ) -> IoResult<()> {
    let pool = pool.clone();
    let resource_kind = resource_kind.to_owned();
    let resource_key = resource_key.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let query = diesel::delete(
        distributed_mutexes::table
          .filter(distributed_mutexes::resource_kind.eq(&resource_kind))
          .filter(distributed_mutexes::resource_key.eq(&resource_key)),
      );
      query.execute(&mut conn).map_err(Self::map_err)?;
      Ok(())
    })
    .await
    .map_err(|err| {
      IoError::interrupted(
        "Interrupted while deleting lock",
        err.to_string().as_str(),
      )
    })?
  }
}
