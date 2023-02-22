use crate::{
  error::HttpResponseError,
  models::{NodeDbModel, Pool},
  repositories,
};

pub async fn register_node(
  name: &str,
  gateway: &str,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  let node = NodeDbModel {
    name: name.to_owned(),
    ip_address: gateway.to_owned(),
  };

  repositories::node::create_if_not_exists(&node, pool).await?;

  Ok(())
}
