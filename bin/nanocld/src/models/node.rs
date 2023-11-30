use tokio::task::JoinHandle;
use serde::{Serialize, Deserialize};

use nanocl_error::io::IoResult;
use nanocl_stubs::generic::GenericFilter;

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
    unimplemented!()
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
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
