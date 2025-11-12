use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::node::{DeleteNodeCargoParams, Node, StartNodeCargoParams};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for nodes
  const NODE_PATH: &'static str = "/nodes";

  /// List existing nodes in the system
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_node().await;
  /// ```
  ///
  pub async fn list_node(&self) -> HttpClientResult<Vec<Node>> {
    let res = self.send_get(Self::NODE_PATH, None::<String>).await?;
    Self::res_json(res).await
  }

  /// Start cargo instances on a node
  ///
  /// ## Arguments
  ///
  /// * `params` - Parameters for starting the cargo
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocl_stubs::node::StartNodeCargoParams;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let params = StartNodeCargoParams {
  ///   cargo_key: "my-cargo".to_string(),
  ///   replicas: 3,
  /// };
  /// let res = client.start_node_cargo(&params).await;
  /// ```
  ///
  pub async fn start_node_cargo(
    &self,
    params: &StartNodeCargoParams,
  ) -> HttpClientResult<()> {
    let path = format!("{}/cargoes/start", Self::NODE_PATH);
    self.send_get(&path, Some(params)).await?;
    Ok(())
  }

  /// Delete cargo instances on a node
  ///
  /// ## Arguments
  ///
  /// * `params` - Parameters for deleting the cargo
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocl_stubs::node::DeleteNodeCargoParams;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let params = DeleteNodeCargoParams {
  ///   cargo_key: "my-cargo".to_string(),
  /// };
  /// let res = client.delete_node_cargo(&params).await;
  /// ```
  ///
  pub async fn delete_node_cargo(
    &self,
    params: &DeleteNodeCargoParams,
  ) -> HttpClientResult<()> {
    let path = format!("{}/cargoes", Self::NODE_PATH);
    self.send_delete(&path, Some(params)).await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use crate::ConnectOpts;

  use super::*;

  #[ntex::test]
  async fn basic() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    let node = client.list_node().await;
    assert!(node.is_ok());
  }
}
