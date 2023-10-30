use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

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

  pub async fn remove_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<Receiver<Result<StateStream, HttpError>>, HttpClientError> {
    let res = self
      .send_put(
        format!("/{}/state/remove", &self.version),
        Some(data),
        None::<String>,
      )
      .await?;

    Ok(Self::res_stream(res).await)
  }
}
