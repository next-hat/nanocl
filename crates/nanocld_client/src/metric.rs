use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::{
  metric::{Metric, MetricPartial},
  generic::GenericFilter,
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for metrics
  const METRIC_PATH: &'static str = "/metrics";

  /// List existing metrics in the system
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_metric(None).await;
  /// ```
  pub async fn list_metric(
    &self,
    filter: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<Metric>> {
    let res = self.send_get(Self::METRIC_PATH, filter).await?;
    Self::res_json(res).await
  }

  /// Create a new metric in the system
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::stubs::metric::MetricPartial;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_metric(&MetricPartial {
  ///  kind: "my-source.io/type".to_owned(),
  ///  data: serde_json::json!({
  ///   "name": "my-metric",
  ///   "description": "My metric",
  ///  }),
  /// }).await;
  /// ```
  pub async fn create_metric(
    &self,
    metric: &MetricPartial,
  ) -> HttpClientResult<Metric> {
    let res = self
      .send_post(Self::METRIC_PATH, Some(metric), None::<String>)
      .await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn basic() {
    let client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
    let metric = client
      .create_metric(&MetricPartial {
        kind: "my-source.io/type".to_owned(),
        data: serde_json::json!({
          "name": "my-metric",
          "description": "My metric",
        }),
      })
      .await
      .unwrap();
    assert_eq!(metric.kind, "my-source.io/type");
    let metrics = client.list_metric(None).await.unwrap();
    assert!(!metrics.is_empty());
  }
}
