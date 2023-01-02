use nanocl_models::system::Version;

use super::{
  http_client::NanoclClient,
  error::{NanoclClientError, is_api_error},
};

impl NanoclClient {
  /// ## Get version
  /// Get daemon version informations
  pub async fn get_version(&self) -> Result<Version, NanoclClientError> {
    let mut res = self.get(String::from("/version")).send().await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;
    let v = res.json::<Version>().await?;

    Ok(v)
  }
}
