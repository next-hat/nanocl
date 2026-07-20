use std::collections::HashMap;

use diesel::{prelude::*, upsert::excluded};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  generic::GenericFilter,
  network::{Network, NetworkPartial},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, NetworkDb, Pool},
  schema::networks,
  utils,
};

use super::generic::*;

impl RepositoryBase for NetworkDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "networks.key")),
      ("name", (ColumnType::Text, "networks.name")),
      ("node_name", (ColumnType::Text, "networks.node_name")),
      ("data", (ColumnType::Json, "networks.data")),
      ("metadata", (ColumnType::Json, "networks.metadata")),
      (
        "created_at",
        (ColumnType::Timestamptz, "networks.created_at"),
      ),
      (
        "updated_at",
        (ColumnType::Timestamptz, "networks.updated_at"),
      ),
    ])
  }
}

impl RepositoryDelByPk for NetworkDb {}

impl RepositoryDelBy for NetworkDb {
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
    let mut query = diesel::delete(networks::table).into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns)
  }
}

impl RepositoryReadBy for NetworkDb {
  type Output = NetworkDb;

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
    let mut query = networks::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(networks::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryReadByTransform for NetworkDb {
  type NewOutput = Network;

  fn transform(input: Self::Output) -> IoResult<Self::NewOutput> {
    input.try_into()
  }
}

impl NetworkDb {
  /// Insert or refresh a Docker network inspection atomically.
  ///
  /// Metadata is accepted for the first insert and deliberately omitted from
  /// the conflict update so user-defined metadata survives daemon refreshes.
  pub async fn upsert(
    network: &NetworkPartial,
    pool: &Pool,
  ) -> IoResult<NetworkDb> {
    let network = NetworkDb::try_from(network)?;
    let pool = pool.clone();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let network = diesel::insert_into(networks::table)
        .values(&network)
        // The readable dotted key is user-facing. Target the independently
        // unique component pair so a theoretical dotted-key collision fails
        // instead of overwriting a different node/network record.
        .on_conflict((networks::node_name, networks::name))
        .do_update()
        .set((
          networks::updated_at.eq(excluded(networks::updated_at)),
          networks::name.eq(excluded(networks::name)),
          networks::node_name.eq(excluded(networks::node_name)),
          networks::data.eq(excluded(networks::data)),
        ))
        .get_result(&mut conn)
        .map_err(NetworkDb::map_err)?;
      Ok(network)
    })
    .await
    .map_err(|err| {
      IoError::interrupted(
        "Interrupted while upserting network",
        &err.to_string(),
      )
    })?
  }
}
