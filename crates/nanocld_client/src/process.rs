use nanocl_error::io::IoError;
use nanocl_stubs::cargo::CargoKillOptions;
use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::{HttpClientResult, HttpClientError};

use nanocl_stubs::generic::{GenericNspQuery, GenericFilter, GenericListQuery};
use nanocl_stubs::process::{
  Process, ProcessLogQuery, ProcessOutputLog, ProcessWaitQuery,
  ProcessWaitResponse,
};

use super::NanocldClient;

impl NanocldClient {
  const PROCESS_PATH: &'static str = "/processes";

  /// List of current processes (vm, job, cargo) managed by the daemon
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_process(None).await;
  /// ```
  pub async fn list_process(
    &self,
    query: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<Process>> {
    let query = query.cloned().unwrap_or_default();
    let query = GenericListQuery::try_from(query).map_err(|err| {
      HttpClientError::IoError(IoError::invalid_data(
        "Query".to_owned(),
        err.to_string(),
      ))
    })?;
    let res = self.send_get(Self::PROCESS_PATH, Some(&query)).await?;
    Self::res_json(res).await
  }

  /// Get logs of a process
  pub async fn logs_process(
    &self,
    kind: &str,
    name: &str,
    query: Option<&ProcessLogQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<ProcessOutputLog>>> {
    let res = self
      .send_get(&format!("{}/{kind}/{name}/logs", Self::PROCESS_PATH), query)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// Start a process by it's kind and name and namespace
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.start_process("cargo", "my-cargo", None).await;
  /// ```
  pub async fn start_process(
    &self,
    kind: &str,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{name}/start", Self::PROCESS_PATH),
        None::<String>,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// Restart a process by it's kind and name and namespace
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.restart_process("cargo", "my-cargo", None).await;
  /// ```
  pub async fn restart_process(
    &self,
    kind: &str,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{name}/restart", Self::PROCESS_PATH),
        None::<String>,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// Stop a process by it's kind and name and namespace
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.stop_cargo("my-cargo", None).await;
  /// ```
  pub async fn stop_process(
    &self,
    kind: &str,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{name}/stop", Self::PROCESS_PATH),
        None::<String>,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// Kill processes by it's kind and name and namespace
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.kill_process("cargo", "my-cargo", None, None).await;
  /// ```
  pub async fn kill_process(
    &self,
    kind: &str,
    name: &str,
    query: Option<&CargoKillOptions>,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{name}/kill", Self::PROCESS_PATH),
        query,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// A stream is returned, data are sent when processes reach status
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let stream = client.wait_process("job", "my_job", None).await.unwrap();
  /// ```
  pub async fn wait_process(
    &self,
    kind: &str,
    name: &str,
    query: Option<&ProcessWaitQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<ProcessWaitResponse>>> {
    let res = self
      .send_get(&format!("{}/{kind}/{name}/wait", Self::PROCESS_PATH), query)
      .await?;
    Ok(Self::res_stream(res).await)
  }
}

#[cfg(test)]
mod tests {
  use crate::ConnectOpts;

  use super::*;

  use futures::StreamExt;

  #[ntex::test]
  async fn logs_cargo() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    });
    let mut rx = client
      .logs_process(
        "cargo",
        "nstore",
        Some(&ProcessLogQuery::of_namespace("system")),
      )
      .await
      .unwrap();
    let _out = rx.next().await.unwrap().unwrap();
  }
}
