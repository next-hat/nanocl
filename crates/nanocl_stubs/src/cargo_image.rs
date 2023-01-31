#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Cargo Image Partial is used to pull a new container image
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoImagePartial {
  /// Name of the image
  pub name: String,
}
