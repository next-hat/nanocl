#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmImage {
  /// The name of the image
  pub name: String,
  /// When the image was created
  pub created_at: chrono::NaiveDateTime,
  /// The path to the image
  pub path: String,
  /// The type of the image
  pub kind: String,
  /// The format of the image
  pub format: String,
  /// The actual size of the image in bytes
  pub size_actual: i64,
  /// The virtual size of the image in bytes
  pub size_virtual: i64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmImageResizePayload {
  /// The new size of the image in bytes
  pub size: u64,
  /// Whether to shrink the image or not
  pub shrink: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum VmImageCloneStream {
  /// The progress of the clone operation
  Progress(f32),
  /// The result of the clone operation
  Done(VmImage),
}
