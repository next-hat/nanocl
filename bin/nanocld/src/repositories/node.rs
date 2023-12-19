use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::GenericFilter;

use crate::{
  models::{Pool, NodeDb},
  schema::nodes,
};

use super::generic::*;

impl RepositoryBase for NodeDb {}

impl RepositoryCreate for NodeDb {}

impl RepositoryDelByPk for NodeDb {}

impl RepositoryRead for NodeDb {
  type Output = NodeDb;
  type Query = nodes::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let mut query = nodes::dsl::nodes.into_boxed();
    if is_multiple {
      query = query.order(nodes::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl NodeDb {
  pub(crate) async fn create_if_not_exists(
    node: &NodeDb,
    pool: &Pool,
  ) -> IoResult<NodeDb> {
    match NodeDb::read_by_pk(&node.name, pool).await? {
      Err(_) => NodeDb::create_from(node.clone(), pool).await?,
      Ok(node) => Ok(node),
    }
  }
}
