use ntex::rt;
use ntex::channel::mpsc;
use ntex::util::BytesMut;
use futures::TryStreamExt;

use nanocl_stubs::system::{Event, Version, HostInfo};

use super::http_client::NanocldClient;
use super::error::{NanocldClientError, is_api_error};

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
    let mut res = self.get(String::from("/version")).send().await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;
    let v = res.json::<Version>().await?;

    Ok(v)
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
  ) -> Result<mpsc::Receiver<Event>, NanocldClientError> {
    let mut res = self
      .get(format!("/{}/events", &self.version))
      .send()
      .await?;
    let status = res.status();
    let (sx, rx) = mpsc::channel::<Event>();
    is_api_error(&mut res, &status).await?;
    rt::spawn(async move {
      let mut buffer = BytesMut::new();
      let mut stream = res.into_stream();
      while let Some(item) = stream.try_next().await.unwrap() {
        buffer.extend_from_slice(&item);
        if item.last() == Some(&b'\n') {
          let event = serde_json::from_slice::<Event>(&buffer).unwrap();
          if sx.send(event).is_err() {
            break;
          }
          buffer.clear();
        }
      }
      sx.close();
    });

    Ok(rx)
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
    let mut res = self.get(String::from("/_ping")).send().await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;
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
    let mut res = self.get(format!("/{}/info", &self.version)).send().await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;
    let info = res.json::<HostInfo>().await?;

    Ok(info)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn test_get_version() {
    let client = NanocldClient::connect_with_unix_default();
    let version = client.get_version().await;

    assert!(version.is_ok());
  }

  #[ntex::test]
  async fn test_watch_events() {
    let client = NanocldClient::connect_with_unix_default();
    let _stream = client.watch_events().await.unwrap();
    // Todo : find a way to test this on CI because it's limited to 2 threads
    // let _event = stream.next().await.unwrap();
  }

  #[ntex::test]
  async fn test_info() {
    let client = NanocldClient::connect_with_unix_default();
    let info = client.info().await.unwrap();

    assert!(info.docker.containers.unwrap() > 0);
  }
}
