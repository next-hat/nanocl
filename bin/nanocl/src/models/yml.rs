use serde::{Serialize, Deserialize};

use nanocl_models::resource::ResourcePartial;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct YmlFile {
  pub(crate) api_version: String,
  pub(crate) r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct YmlResource {
  pub(crate) resources: Vec<ResourcePartial>,
}
