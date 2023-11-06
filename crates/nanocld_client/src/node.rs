use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::node::Node;

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for nodes
  const NODE_PATH: &'static str = "/nodes";

  /// ## List node
  ///
  /// List existing nodes in the system
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [node](Node) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
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
  pub async fn list_node(&self) -> Result<Vec<Node>, HttpClientError> {
    let res = self.send_get(Self::NODE_PATH, None::<String>).await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn basic() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let node = client.list_node().await;
    assert!(node.is_ok());
  }
}
