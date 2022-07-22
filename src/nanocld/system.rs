use serde::{Serialize, Deserialize};

use super::client::Nanocld;
use super::error::{NanocldError, is_api_error};

#[derive(Serialize, Deserialize)]
pub struct Version {
  pub(crate) arch: String,
  pub(crate) commit_id: String,
  pub(crate) version: String,
}

impl Nanocld {

  /// # Get version
  /// Get daemon version informations
  pub async fn get_version(&self) -> Result<Version, NanocldError> {
    let mut res = self.get(String::from("/version")).send().await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;
    let v = res.json::<Version>().await?;

    Ok(v)
  }
}
