use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::node::Node;

use super::http_client::NanocldClient;

impl NanocldClient {
  pub async fn list_node(&self) -> Result<Vec<Node>, HttpClientError> {
    let res = self
      .send_get(format!("/{}/nodes", &self.version), None::<String>)
      .await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn basic() {
    let client = NanocldClient::connect_to("http://localhost:8585", None);
    let node = client.list_node().await;
    assert!(node.is_ok());
  }
}
