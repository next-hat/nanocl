use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::node::Node;

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
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn basic() {
    let client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
    let node = client.list_node().await;
    assert!(node.is_ok());
  }
}
