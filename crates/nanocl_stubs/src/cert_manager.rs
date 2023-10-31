use serde::{Serialize, Deserialize};

use crate::cargo_config::CargoConfigPartial;

/// Defines a CertManagerIssuer config
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CertManagerIssuer {
  /// Certificate generation CargoConfig
  pub generate: CargoConfigPartial,
}
