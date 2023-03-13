use diesel::prelude::*;

use nanocl_stubs::vm_image::VmImage;
use serde::{Serialize, Deserialize};

use crate::schema::vm_images;

#[derive(
  Clone, Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = vm_images)]
#[serde(rename_all = "PascalCase")]
pub struct VmImageDbModel {
  pub(crate) name: String,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) kind: String,
  pub(crate) path: String,
  pub(crate) format: String,
  pub(crate) size_actual: i64,
  pub(crate) size_virtual: i64,
  pub(crate) parent: Option<String>,
}

#[derive(Clone, Debug, AsChangeset)]
#[diesel(table_name = vm_images)]
pub struct VmImageUpdateDbModel {
  pub(crate) size_actual: i64,
  pub(crate) size_virtual: i64,
}

impl From<VmImageDbModel> for VmImage {
  fn from(db_model: VmImageDbModel) -> Self {
    Self {
      name: db_model.name,
      created_at: db_model.created_at,
      path: db_model.path,
      kind: db_model.kind,
      format: db_model.format,
      size_actual: db_model.size_actual,
      size_virtual: db_model.size_virtual,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct QemuImgInfo {
  pub(crate) format: String,
  pub(crate) virtual_size: i64,
  pub(crate) actual_size: i64,
}
