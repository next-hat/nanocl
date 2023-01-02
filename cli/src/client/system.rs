use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

impl Nanocld {
  /// ## Get version
  /// Get daemon version informations
  pub async fn get_version(&self) -> Result<Version, NanocldError> {
    let mut res = self.get(String::from("/version")).send().await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;
    let v = res.json::<Version>().await?;

    Ok(v)
  }
}
