use nanocl_error::http_client::HttpClientResult;
use nanocl_stubs::{generic::GenericFilter, network::Network};

use super::NanocldClient;

impl NanocldClient {
  const NETWORK_PATH: &'static str = "/networks";

  /// List persisted node-scoped Docker networks.
  pub async fn list_network(
    &self,
    query: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<Network>> {
    let query = Self::convert_query(query)?;
    let res = self.send_get(Self::NETWORK_PATH, Some(&query)).await?;
    Self::res_json(res).await
  }

  /// Inspect a persisted Docker network using its deterministic
  /// `{node_name}.{network_name}` key.
  pub async fn inspect_network(&self, key: &str) -> HttpClientResult<Network> {
    let res = self
      .send_get(
        &format!("{}/{key}/inspect", Self::NETWORK_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use crate::ConnectOpts;

  use super::*;

  #[ntex::test]
  async fn list_and_inspect_networks() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocld client");
    let networks = client.list_network(None).await.unwrap();
    if let Some(network) = networks.first() {
      let inspected = client.inspect_network(&network.key).await.unwrap();
      assert_eq!(inspected.key, network.key);
    }
  }
}
