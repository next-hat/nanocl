#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmImage {
  pub name: String,
  pub created_at: chrono::NaiveDateTime,
  pub path: String,
  pub kind: String,
  pub format: String,
  pub size_actual: i64,
  pub size_virtual: i64,
}
