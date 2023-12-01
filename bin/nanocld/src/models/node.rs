use std::sync::Arc;

use diesel::prelude::*;
use tokio::task::JoinHandle;
use serde::{Serialize, Deserialize};

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::generic::GenericFilter;

use crate::utils;
use crate::schema::nodes;

use super::{Pool, Repository};

/// This structure represent a node in the database.
/// A node is a machine that is connected to nanocl network.
#[derive(
  Debug, Clone, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = nodes)]
#[serde(rename_all = "PascalCase")]
pub struct NodeDb {
  /// The name of the node
  pub name: String,
  /// The ip address of the node
  pub ip_address: String,
}

impl Repository for NodeDb {
  type Table = nodes::table;
  type Item = NodeDb;
  type UpdateItem = NodeDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::debug!("NodeDb::find_one filter: {filter:?}");
    // let r#where = filter.r#where.to_owned().unwrap_or_default();
    let query = nodes::dsl::nodes.into_boxed();
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::debug!("NodeDb::find filter: {filter:?}");
    // let r#where = filter.r#where.to_owned().unwrap_or_default();
    let query = nodes::dsl::nodes.into_boxed();
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(items)
    })
  }
}

impl NodeDb {
  pub(crate) async fn create_if_not_exists(
    node: &NodeDb,
    pool: &Pool,
  ) -> IoResult<NodeDb> {
    match NodeDb::find_by_pk(&node.name, pool).await? {
      Err(_) => NodeDb::create(node.clone(), pool).await?,
      Ok(node) => Ok(node),
    }
  }
}
