use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Cargo Image Partial is used to pull a new container image
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoImagePartial {
  /// Name of the image
  #[cfg_attr(feature = "utoipa", schema(example = "nginx:latest"))]
  pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ListCargoImagesOptions {
  /// Show all images. Only images from a final layer (no children) are shown by default.
  pub all: Option<bool>,
  /// A JSON encoded value of the filters to process on the images list. Available filters:
  ///  - `before`=(`<image-name>[:<tag>]`, `<image id>` or `<image@digest>`)
  ///  - `dangling`=`true`
  ///  - `label`=`key` or `label`=`"key=value"` of an image label
  ///  - `reference`=(`<image-name>[:<tag>]`)
  ///  - `since`=(`<image-name>[:<tag>]`, `<image id>` or `<image@digest>`)
  pub filters: Option<HashMap<String, Vec<String>>>,
  /// Show digest information as a RepoDigests field on each image.
  pub digests: Option<bool>,
}

impl From<ListCargoImagesOptions>
  for bollard_next::image::ListImagesOptions<String>
{
  fn from(options: ListCargoImagesOptions) -> Self {
    Self {
      all: options.all.unwrap_or_default(),
      filters: options.filters.unwrap_or_default(),
      digests: options.digests.unwrap_or_default(),
    }
  }
}

/// Cargo Image is used to pull a new container image from a tar archive
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoImageImportOptions {
  /// Show progress during import
  pub quiet: Option<bool>,
}
