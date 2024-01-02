use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::system::{Event, BinaryInfo, HostInfo};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// Get the version of the daemon
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.get_version().await;
  /// ```
  pub async fn get_version(&self) -> HttpClientResult<BinaryInfo> {
    let res = self.send_get("/version", None::<String>).await?;
    Self::res_json(res).await
  }

  /// Watch daemon events
  /// It will emit an event when the daemon state change
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
  pub async fn watch_events(
    &self,
  ) -> HttpClientResult<Receiver<HttpResult<Event>>> {
    let res = self.send_get("/events/watch", None::<String>).await?;
    Ok(Self::res_stream(res).await)
  }

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
  pub async fn ping(&self) -> HttpClientResult<()> {
    self.send_head("/_ping", None::<String>).await?;
    Ok(())
  }

  /// Get details about the host and docker daemon
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let info = client.info().await.unwrap();
  /// ```
  pub async fn info(&self) -> HttpClientResult<HostInfo> {
    let res = self.send_get("/info", None::<String>).await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn get_version() {
    let client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
    let version = client.get_version().await;
    assert!(version.is_ok());
  }

  #[ntex::test]
  async fn watch_events() {
    let client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
    let _stream = client.watch_events().await.unwrap();
    // Todo : find a way to test this on CI because it's limited to 2 threads
    // let _event = stream.next().await.unwrap();
  }

  #[ntex::test]
  async fn info() {
    let client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
    let info = client.info().await.unwrap();
    assert!(info.docker.containers.unwrap() > 0);
  }
}
