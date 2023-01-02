use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Version {
  pub(crate) arch: String,
  pub(crate) commit_id: String,
  pub(crate) version: String,
}
