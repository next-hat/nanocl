use nanocl_stubs::http_metric::{HttpMetric, HttpMetricListQuery};
use nanocl_error::http_client::HttpClientError;

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for http metrics
  const HTTP_METRIC_PATH: &'static str = "/http_metrics";

  /// ## List http metrics
  ///
  /// List http metric from the system
  ///
  /// ## Arguments
  ///
  /// * [query](Option) - The optional [query](HttpMetricListQuery)
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [http metrics](HttpMetric) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_http_metric(None).await;
  /// ```
  ///
  pub async fn list_http_metric(
    &self,
    query: Option<&HttpMetricListQuery>,
  ) -> Result<Vec<HttpMetric>, HttpClientError> {
    let res = self.send_get(Self::HTTP_METRIC_PATH, query).await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_error::http_client::HttpClientError;

  #[ntex::test]
  async fn list_metric() -> Result<(), HttpClientError> {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let res = client.list_http_metric(None).await;
    assert!(res.is_ok());
    Ok(())
  }
}
