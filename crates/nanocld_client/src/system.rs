use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::system::{Event, BinaryInfo, HostInfo, ProccessQuery};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Get version
  ///
  /// Get the version of the daemon
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Version](Version)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.get_version().await;
  /// ```
  ///
  pub async fn get_version(&self) -> HttpClientResult<BinaryInfo> {
    let res = self.send_get("/version", None::<String>).await?;
    Self::res_json(res).await
  }

  /// ## Watch events
  ///
  /// Watch daemon events
  /// It will emit an event when the daemon state change
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Receiver](Receiver) of [Event](Event)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let mut stream = client.watch_events().await?;
  /// while let Some(event) = stream.next().await {
  ///  println!("{:?}", event);
  /// }
  /// ```
  ///
  pub async fn watch_events(
    &self,
  ) -> HttpClientResult<Receiver<HttpResult<Event>>> {
    let res = self.send_get("/events", None::<String>).await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Ping the daemon
  ///
  /// Check if the daemon is running
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.ping().await;
  /// ```
  ///
  pub async fn ping(&self) -> HttpClientResult<()> {
    self.send_head("/_ping", None::<String>).await?;
    Ok(())
  }

  /// ## Get the host info
  ///
  /// Get details about the host and docker daemon
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [HostInfo](HostInfo)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let info = client.info().await.unwrap();
  /// ```
  ///
  pub async fn info(&self) -> HttpClientResult<HostInfo> {
    let res = self.send_get("/info", None::<String>).await?;
    Self::res_json(res).await
  }

  /// ## Process
  ///
  /// List of current processes (vm, cargoes) managed by the daemon
  ///
  /// ## Arguments
  ///
  /// * [opts](Option) - The optional [query](ProccessQuery)
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vec](Vec) of [NodeContainerSummary](NodeContainerSummary)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.process(None).await;
  /// ```
  ///
  pub async fn process(
    &self,
    opts: Option<&ProccessQuery>,
  ) -> HttpClientResult<Vec<NodeContainerSummary>> {
    let res = self.send_get("/processes", opts).await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn get_version() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let version = client.get_version().await;
    assert!(version.is_ok());
  }

  #[ntex::test]
  async fn watch_events() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let _stream = client.watch_events().await.unwrap();
    // Todo : find a way to test this on CI because it's limited to 2 threads
    // let _event = stream.next().await.unwrap();
  }

  #[ntex::test]
  async fn info() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let info = client.info().await.unwrap();
    assert!(info.docker.containers.unwrap() > 0);
  }
}
