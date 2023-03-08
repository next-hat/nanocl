use crate::http_client::NanocldClient;

use crate::error::NanocldClientError;

impl NanocldClient {
  pub async fn apply_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<(), NanocldClientError> {
    self
      .send_put(
        format!("/{}/state/apply", &self.version),
        Some(data),
        None::<String>,
      )
      .await?;

    Ok(())
  }

  pub async fn revert_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<(), NanocldClientError> {
    self
      .send_put(
        format!("/{}/state/revert", &self.version),
        Some(data),
        None::<String>,
      )
      .await?;

    Ok(())
  }
}
