use ntex::rt;
use ntex::channel::mpsc;
use ntex::util::BytesMut;
use futures::TryStreamExt;

use nanocl_stubs::system::{Event, Version};

use super::http_client::NanoclClient;
use super::error::{NanoclClientError, is_api_error};

impl NanoclClient {
  /// ## Get the version of the daemon
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The [version](Version) of the daemon
  ///   * [Err](NanoclClientError) - The version could not be retrieved
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let version = client.get_version().await;
  /// ```
  ///
  pub async fn get_version(&self) -> Result<Version, NanoclClientError> {
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
  ///   * [Err](NanoclClientError) - The events could not be retrieved
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let mut stream = client.watch_events().await?;
  /// while let Some(event) = stream.next().await {
  ///  println!("{:?}", event);
  /// }
  /// ```
  ///
  pub async fn watch_events(
    &self,
  ) -> Result<mpsc::Receiver<Event>, NanoclClientError> {
    let mut res = self.get(String::from("/events")).send().await?;
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
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn test_get_version() {
    let client = NanoclClient::connect_with_unix_default().await;
    let version = client.get_version().await;

    assert!(version.is_ok());
  }

  #[ntex::test]
  async fn test_watch_events() {
    let client = NanoclClient::connect_with_unix_default().await;
    let _stream = client.watch_events().await.unwrap();
    // Todo : find a way to test this on CI because it's limited to 2 threads
    // let _event = stream.next().await.unwrap();
  }
}
