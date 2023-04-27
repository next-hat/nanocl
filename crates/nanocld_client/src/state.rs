use ntex::channel::mpsc::Receiver;

use nanocl_utils::http_error::HttpError;
use nanocl_utils::http_client_error::HttpClientError;

use nanocl_stubs::state::StateStream;

use crate::http_client::NanocldClient;

impl NanocldClient {
  pub async fn apply_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<Receiver<Result<StateStream, HttpError>>, HttpClientError> {
    let res = self
      .send_put(
        format!("/{}/state/apply", &self.version),
        Some(data),
        None::<String>,
      )
      .await?;

    Ok(Self::res_stream(res).await)
  }

  pub async fn revert_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<Receiver<Result<StateStream, HttpError>>, HttpClientError> {
    let res = self
      .send_put(
        format!("/{}/state/revert", &self.version),
        Some(data),
        None::<String>,
      )
      .await?;

    Ok(Self::res_stream(res).await)
  }
}
