use ntex::channel::mpsc::Receiver;

use nanocl_error::{http::HttpResult, http_client::HttpClientResult};

use nanocl_stubs::{
  cargo::CargoKillOptions,
  generic::GenericFilter,
  process::{
    Process, ProcessLogQuery, ProcessOutputLog, ProcessStats,
    ProcessStatsQuery, ProcessWaitQuery, ProcessWaitResponse,
  },
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
  ///
  pub async fn list_process(
    &self,
    query: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<Process>> {
    let query = Self::convert_query(query)?;
    let res = self.send_get(Self::PROCESS_PATH, Some(&query)).await?;
    Self::res_json(res).await
  }

  /// Get Log of a single process by it's name or id
  /// Cargoes, jobs, can have multiple instances, this endpoint get logs of a single instance
  ///
  pub async fn logs_process(
    &self,
    name: &str,
    query: Option<&ProcessLogQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<ProcessOutputLog>>> {
    let res = self
      .send_get(&format!("{}/{name}/logs", Self::PROCESS_PATH), query)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// Get logs of processes for a specific object
  /// Cargoes, jobs, can have multiple instances, this endpoint get logs all instances
  ///
  pub async fn logs_processes(
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

  /// Start all processes for a kind and canonical resource key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.start_process("cargo", "global.my-cargo").await;
  /// ```
  ///
  pub async fn start_process(
    &self,
    kind: &str,
    key: &str,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{key}/start", Self::PROCESS_PATH),
        None::<String>,
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// Restart all processes for a kind and canonical resource key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.restart_process("cargo", "global.my-cargo").await;
  /// ```
  ///
  pub async fn restart_process(
    &self,
    kind: &str,
    key: &str,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{key}/restart", Self::PROCESS_PATH),
        None::<String>,
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// Stop all processes for a kind and canonical resource key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.stop_process("cargo", "global.my-cargo").await;
  /// ```
  ///
  pub async fn stop_process(
    &self,
    kind: &str,
    key: &str,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{key}/stop", Self::PROCESS_PATH),
        None::<String>,
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// Kill all processes for a kind and canonical resource key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.kill_process("cargo", "global.my-cargo", None).await;
  /// ```
  ///
  pub async fn kill_process(
    &self,
    kind: &str,
    key: &str,
    query: Option<&CargoKillOptions>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{kind}/{key}/kill", Self::PROCESS_PATH),
        query,
        None::<String>,
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
  ///
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

  /// The stats are streamed as a [Receiver](Receiver) of [stats](Stats)
  ///
  pub async fn stats_processes(
    &self,
    kind: &str,
    name: &str,
    query: Option<&ProcessStatsQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<ProcessStats>>> {
    let res = self
      .send_get(
        &format!("{}/{kind}/{name}/stats", Self::PROCESS_PATH),
        query,
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }
  /// The stats are streamed as a [Receiver](Receiver) of [stats](Stats)
  /// It get the stats by key name(returned by nanocl ps) instead of name and kind
  /// ## Example
  ///
  /// ```no_run, ignore
  /// use nanocld_client::NanocldClient;
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let stats = client.stats_process_by_name("system.nstore.c");
  /// ```
  ///
  pub async fn stats_process_by_name(
    &self,
    name: &str,
  ) -> HttpClientResult<Receiver<HttpResult<ProcessStats>>> {
    let query: Option<&ProcessStatsQuery> = None;
    let res = self
      .send_get(&format!("/process/{name}/stats"), query)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// Inspect a process by it's name
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let process = client.inspect_process("system.nstore.c").await.unwrap();
  /// ```
  ///
  pub async fn inspect_process(&self, name: &str) -> HttpClientResult<Process> {
    let res = self
      .send_get(
        &format!("{}/{name}/inspect", Self::PROCESS_PATH),
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

  use futures::StreamExt;

  #[ntex::test]
  async fn logs_process() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    let mut rx = client
      .logs_processes(
        "cargo",
        "system.nstore",
        Some(&ProcessLogQuery::default()),
      )
      .await
      .unwrap();
    let _out = rx.next().await.unwrap().unwrap();
  }

  #[ntex::test]
  async fn process_stats_by_name() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    let mut rx = client
      .stats_process_by_name("system.nstore.c")
      .await
      .unwrap();
    let _out = rx.next().await.unwrap().unwrap();
  }

  #[ntex::test]
  async fn inspect_process() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    let _out = client.inspect_process("system.nstore.c").await.unwrap();
  }
}
