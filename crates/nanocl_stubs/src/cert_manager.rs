use serde::{Serialize, Deserialize};

use crate::cargo_spec::CargoSpecPartial;

/// Defines a CertManagerIssuer spec
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CertManagerIssuer {
  /// Certificate generation CargoSpec
  pub generate: CargoSpecPartial,
}
