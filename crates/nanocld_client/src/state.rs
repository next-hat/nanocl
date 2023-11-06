use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::state::StateStream;

use crate::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for state
  const STATE_PATH: &'static str = "/state";

  /// ## Apply state
  ///
  /// Apply a state to the system
  ///
  /// ## Arguments
  ///
  /// * [data](serde_json::Value) - The state to apply
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - A [stream](Receiver) of result of [state stream](StateStream) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  pub async fn apply_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<Receiver<Result<StateStream, HttpError>>, HttpClientError> {
    let res = self
      .send_put(
        &format!("{}/apply", Self::STATE_PATH),
        Some(data),
        None::<String>,
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Remove state
  ///
  /// Remove a state from the system
  ///
  /// ## Arguments
  ///
  /// * [data](serde_json::Value) - The state to remove
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - A [stream](Receiver) of result of [state stream](StateStream) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  pub async fn remove_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<Receiver<Result<StateStream, HttpError>>, HttpClientError> {
    let res = self
      .send_put(
        &format!("{}/remove", Self::STATE_PATH),
        Some(data),
        None::<String>,
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }
}
