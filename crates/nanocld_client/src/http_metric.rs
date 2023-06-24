use nanocl_stubs::http_metric::{HttpMetric, HttpMetricListQuery};
use nanocl_utils::http_client_error::HttpClientError;

use super::http_client::NanocldClient;

impl NanocldClient {
  pub async fn list_http_metric(
    &self,
    query: Option<HttpMetricListQuery>,
  ) -> Result<Vec<HttpMetric>, HttpClientError> {
    let res = self
      .send_get(format!("/{}/http_metrics", &self.version), query)
      .await?;

    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_utils::http_client_error::HttpClientError;

  #[ntex::test]
  async fn list_metric() -> Result<(), HttpClientError> {
    let client = NanocldClient::connect_to("http://localhost:8585", None);
    let res = client.list_http_metric(None::<HttpMetricListQuery>).await;
    assert!(res.is_ok());
    Ok(())
  }
}
