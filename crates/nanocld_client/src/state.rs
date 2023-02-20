use crate::http_client::NanocldClient;

use crate::error::{NanocldClientError, is_api_error};

impl NanocldClient {
  pub async fn apply_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<(), NanocldClientError> {
    let mut res = self.put("/state/apply".into()).send_json(data).await?;

    let status = res.status();

    is_api_error(&mut res, &status).await?;
    Ok(())
  }

  pub async fn revert_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<(), NanocldClientError> {
    let mut res = self.put("/state/revert".into()).send_json(data).await?;

    let status = res.status();

    is_api_error(&mut res, &status).await?;
    Ok(())
  }
}
