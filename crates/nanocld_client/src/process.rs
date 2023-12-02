use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::process::{ProcessLogQuery, ProcessOutputLog};

use super::NanocldClient;

impl NanocldClient {
  const INSTANCE_PATH: &'static str = "/processes";

  /// Get logs of a process
  pub async fn logs_process(
    &self,
    kind: &str,
    name: &str,
    query: Option<&ProcessLogQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<ProcessOutputLog>>> {
    let res = self
      .send_get(
        &format!("{}/{kind}/{name}/logs", Self::INSTANCE_PATH),
        query,
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }
}
