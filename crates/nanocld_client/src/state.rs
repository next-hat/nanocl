use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::state::{StateStream, StateApplyQuery};

use crate::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for state
  const STATE_PATH: &'static str = "/state";

  /// Apply a state to the system
  pub async fn apply_state(
    &self,
    data: &serde_json::Value,
    options: Option<&StateApplyQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<StateStream>>> {
    let res = self
      .send_put(&format!("{}/apply", Self::STATE_PATH), Some(data), options)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// Remove a state from the system
  pub async fn remove_state(
    &self,
    data: &serde_json::Value,
  ) -> HttpClientResult<Receiver<HttpResult<StateStream>>> {
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
