use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::GenericFilter;

use crate::{
  gen_multiple, gen_where4string,
  models::{NodeDb, Pool, SystemState},
  schema::nodes,
};

use super::generic::*;

impl RepositoryBase for NodeDb {}

impl RepositoryCreate for NodeDb {}

impl RepositoryDelByPk for NodeDb {}

impl RepositoryReadBy for NodeDb {
  type Output = NodeDb;

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
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = nodes::table.into_boxed();
    if let Some(name) = r#where.get("name") {
      gen_where4string!(query, nodes::name, name);
    }
    if is_multiple {
      gen_multiple!(query, nodes::created_at, filter);
    }
    query
  }
}

impl NodeDb {
  pub async fn create_if_not_exists(
    node: &NodeDb,
    pool: &Pool,
  ) -> IoResult<NodeDb> {
    match NodeDb::read_by_pk(&node.name, pool).await {
      Err(_) => NodeDb::create_from(node.clone(), pool).await,
      Ok(node) => Ok(node),
    }
  }

  pub async fn register(state: &SystemState) -> IoResult<()> {
    let node = NodeDb {
      name: state.config.hostname.clone(),
      ip_address: state.config.gateway.clone(),
      created_at: chrono::Utc::now().naive_utc(),
    };
    NodeDb::create_if_not_exists(&node, &state.pool).await?;
    Ok(())
  }
}
