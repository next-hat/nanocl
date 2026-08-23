use ntex::{channel::mpsc::Receiver, io, rt, ws};

use nanocl_error::{
  http::HttpResult, http_client::HttpClientResult, io::FromIo,
};

use nanocl_stubs::{
  generic::GenericFilter,
  process::{
    Process, ProcessLogQuery, ProcessOutputLog, ProcessStats,
    ProcessStatsQuery, ProcessWaitQuery, ProcessWaitResponse,
  },
};

use super::NanocldClient;

impl NanocldClient {
  const PROCESS_PATH: &'static str = "/processes";

  fn process_attach_url(&self, name: &str) -> String {
    self.gen_url(&format!("{}/{name}/attach", Self::PROCESS_PATH))
  }

  /// Attach to a process by its concrete Docker-backed name or ID.
  /// Returns a WebSocket stream for sending input and receiving process output.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::{ConnectOpts, NanocldClient};
  ///
  /// let client = NanocldClient::connect_to(&ConnectOpts::default()).unwrap();
  /// let res = client.attach_process("global.my-vm.v").await;
  /// ```
  pub async fn attach_process(
    &self,
    name: &str,
  ) -> HttpClientResult<ws::WsConnection<io::Base>> {
    let url = self.process_attach_url(name);
    // Open a WebSocket connection over the configured HTTP transport.
    #[cfg(not(target_os = "windows"))]
    {
      use nanocl_error::io::IoError;

      let con = match &self.unix_socket {
        Some(path) => ws::WsClient::builder(&url)
          .connector::<_, _>(ntex::service::fn_service(|_| async move {
            Ok(rt::unix_connect(&path, ntex::SharedCfg::default()).await?)
          }))
          .build(ntex::SharedCfg::default())
          .await
          .map_err(|err| {
            IoError::interrupted(
              "Unable to build websocket client",
              &format!("{err:?}"),
            )
          })?
          .connect()
          .await
          .map_err(|err| err.map_err_context(|| path))?,
        None => ws::WsClient::builder(&url)
          .build(ntex::SharedCfg::default())
          .await
          .map_err(|err| {
            IoError::interrupted(
              "Unable to build websocket client",
              &format!("{err:?}"),
            )
          })?
          .connect()
          .await
          .map_err(|err| err.map_err_context(|| &self.url))?,
      };
      Ok(con)
    }
    #[cfg(target_os = "windows")]
    {
      let con = ws::WsClient::builder(&url)
        .map_err(|err| err.map_err_context(|| &self.url))?
        .connect()
        .await
        .map_err(|err| err.map_err_context(|| &self.url))?;
      Ok(con)
    }
  }

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

  #[test]
  fn process_attach_url_uses_process_route() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");

    assert_eq!(
      client.process_attach_url("global.my-vm.v"),
      "http://nanocl.internal:8585/v0.18.0/processes/global.my-vm.v/attach"
    );
  }

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
