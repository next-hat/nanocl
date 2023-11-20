use ntex::channel::mpsc;

use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::system::{Event, Version, HostInfo, ProccessQuery};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Get the version of the daemon
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Version](Version) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let version = client.get_version().await;
  /// ```
  ///
  pub async fn get_version(&self) -> Result<Version, HttpClientError> {
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
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - A [Receiver](mpsc::Receiver) of [events](Event) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
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
  ) -> Result<mpsc::Receiver<Result<Event, HttpError>>, HttpClientError> {
    let res = self.send_get("/events", None::<String>).await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Ping the daemon
  ///
  /// Check if the daemon is running
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - If operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let version = client.ping().await.unwrap();
  /// ```
  ///
  pub async fn ping(&self) -> Result<(), HttpClientError> {
    self.send_head("/_ping", None::<String>).await?;
    Ok(())
  }

  /// ## Get the host info
  ///
  /// Get details about the host and docker daemon
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [HostInfo](HostInfo) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
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
  pub async fn info(&self) -> Result<HostInfo, HttpClientError> {
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
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [node container summary](NodeContainerSummary) if operation succeeded
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let processes = client.process(None).await;
  /// ```
  ///
  pub async fn process(
    &self,
    opts: Option<&ProccessQuery>,
  ) -> Result<Vec<NodeContainerSummary>, HttpClientError> {
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
