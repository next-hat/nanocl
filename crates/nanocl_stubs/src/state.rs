#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::cargo_config::CargoConfigPartial;

use super::resource::ResourcePartial;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateMeta {
  pub api_version: String,
  pub r#type: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateResources {
  pub resources: Vec<ResourcePartial>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateCargo {
  pub namespace: Option<String>,
  pub cargoes: Vec<CargoConfigPartial>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateDeployment {
  pub namespace: Option<String>,
  pub resources: Option<Vec<ResourcePartial>>,
  pub cargoes: Option<Vec<CargoConfigPartial>>,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum StateStream {
  Msg(String),
  Error(String),
}
