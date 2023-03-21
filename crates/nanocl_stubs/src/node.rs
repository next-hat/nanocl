use bollard_next::service::ContainerSummary;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Node {
  pub name: String,
  pub ip_address: String,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct NodeContainerSummary {
  pub node: String,
  pub ip_address: String,
  pub container: ContainerSummary,
}

impl NodeContainerSummary {
  pub fn new(
    node: String,
    ip_address: String,
    container: ContainerSummary,
  ) -> Self {
    Self {
      node,
      ip_address,
      container,
    }
  }
}
