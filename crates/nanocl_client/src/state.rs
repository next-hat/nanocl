use crate::http_client::NanoclClient;

use crate::error::{NanoclClientError, is_api_error};

impl NanoclClient {
  pub async fn apply_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .put("/state/apply".into())
      .send_body(data.to_string())
      .await?;

    let status = res.status();

    is_api_error(&mut res, &status).await?;
    Ok(())
  }

  pub async fn revert_state(
    &self,
    data: &serde_json::Value,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .put("/state/revert".into())
      .send_body(data.to_string())
      .await?;

    let status = res.status();

    is_api_error(&mut res, &status).await?;
    Ok(())
  }
}
