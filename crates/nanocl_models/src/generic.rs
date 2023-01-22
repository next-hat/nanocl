#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Generic namespace query filter
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct GenericNspQuery {
  /// Name of the namespace
  pub namespace: Option<String>,
}

/// Generic delete response
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
#[cfg_attr(feature = "dev", derive(ToSchema))]
pub struct GenericDelete {
  /// Number of deleted items
  pub count: usize,
}
