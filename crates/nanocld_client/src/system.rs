use bollard_next::service::ContainerSummary;
use ntex::channel::mpsc;

use nanocl_stubs::system::{Event, Version, HostInfo, ProccessQuery};

use crate::error::ApiError;

use super::error::NanocldClientError;
use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Get the version of the daemon
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The [version](Version) of the daemon
  ///   * [Err](NanocldClientError) - The version could not be retrieved
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let version = client.get_version().await;
  /// ```
  ///
  pub async fn get_version(&self) -> Result<Version, NanocldClientError> {
    let res = self.send_get("/version".into(), None::<String>).await?;

    Self::res_json(res).await
  }

  /// ## Watch events
  ///
  /// Watch daemon events
  /// It will emit an event when the daemon state change
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - A [Receiver](mpsc::Receiver) of [Event](Event)s
  ///   * [Err](NanocldClientError) - The events could not be retrieved
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let mut stream = client.watch_events().await?;
  /// while let Some(event) = stream.next().await {
  ///  println!("{:?}", event);
  /// }
  /// ```
  ///
  pub async fn watch_events(
    &self,
  ) -> Result<mpsc::Receiver<Result<Event, ApiError>>, NanocldClientError> {
    let res = self
      .send_get(format!("/{}/events", &self.version), None::<String>)
      .await?;

    Ok(Self::res_stream(res).await)
  }

  /// ## Ping the daemon
  ///
  /// Check if the daemon is running
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The daemon is running
  ///   * [Err](NanocldClientError) - The daemon is not running
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let version = client.ping().await.unwrap();
  /// ```
  ///
  pub async fn ping(&self) -> Result<(), NanocldClientError> {
    self.send_get("/_ping".into(), None::<String>).await?;

    Ok(())
  }

  /// ## Get the host info
  ///
  /// Get details about the host and docker daemon
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The [HostInfo](HostInfo)
  ///   * [Err](NanocldClientError) - The host info could not be retrieved
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let info = client.info().await.unwrap();
  /// ```
  ///
  pub async fn info(&self) -> Result<HostInfo, NanocldClientError> {
    let res = self
      .send_get(format!("/{}/info", &self.version), None::<String>)
      .await?;

    Self::res_json(res).await
  }

  pub async fn process(
    &self,
    opts: Option<ProccessQuery>,
  ) -> Result<Vec<ContainerSummary>, NanocldClientError> {
    let res = self
      .send_get(format!("/{}/processes", &self.version), opts)
      .await?;

    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn get_version() {
    let client = NanocldClient::connect_with_unix_default();
    let version = client.get_version().await;

    assert!(version.is_ok());
  }

  #[ntex::test]
  async fn watch_events() {
    let client = NanocldClient::connect_with_unix_default();
    let _stream = client.watch_events().await.unwrap();
    // Todo : find a way to test this on CI because it's limited to 2 threads
    // let _event = stream.next().await.unwrap();
  }

  #[ntex::test]
  async fn info() {
    let client = NanocldClient::connect_with_unix_default();
    let info = client.info().await.unwrap();

    assert!(info.docker.containers.unwrap() > 0);
  }
}
