use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Version {
  pub arch: String,
  pub version: String,
  pub commit_id: String,
}
