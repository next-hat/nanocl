use serde::{Serialize, Deserialize};

use nanocl_stubs::vm_image::VmImage;

use crate::schema::vm_images;

/// ## VmImageDb
///
/// This structure represent a virtual machine image in the database.
/// A virtual machine image is a file that represent a virtual machine disk.
///
/// Two kind of virtual machine image are supported:
/// - Base: A base image is a virtual machine image that is not based on another image.
/// - Snapshot: A snapshot image is a virtual machine image that is based on a base image.
///
/// A `Snapshot` of a `Base` image will alway be use to create a virtual machine.
///
#[derive(
  Clone, Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = vm_images)]
#[serde(rename_all = "PascalCase")]
pub struct VmImageDb {
  /// The name of the virtual machine image
  pub(crate) name: String,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The kind of the virtual machine image (Base, Snapshot)
  pub(crate) kind: String,
  /// The path of the virtual machine image
  pub(crate) path: String,
  /// The format of the virtual machine image
  pub(crate) format: String,
  /// The actual size of the virtual machine image
  pub(crate) size_actual: i64,
  /// The virtual size of the virtual machine image
  pub(crate) size_virtual: i64,
  /// The parent of the virtual machine image
  pub(crate) parent: Option<String>,
}

/// ## VmImageUpdateDb
///
/// This structure is used to update a virtual machine image in the database.
///
#[derive(Clone, Debug, AsChangeset)]
#[diesel(table_name = vm_images)]
pub struct VmImageUpdateDb {
  /// The actual size of the virtual machine image
  pub(crate) size_actual: i64,
  /// The virtual size of the virtual machine image
  pub(crate) size_virtual: i64,
}

/// ## QemuImgInfo
///
/// This structure is used to parse the output of the qemu-img info command.
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct QemuImgInfo {
  /// The format of the virtual machine image
  pub(crate) format: String,
  /// The virtual size of the virtual machine image
  pub(crate) virtual_size: i64,
  /// The actual size of the virtual machine image
  pub(crate) actual_size: i64,
}

/// Helper to convert a `VmImageDb` to a `VmImage`
impl From<VmImageDb> for VmImage {
  fn from(db: VmImageDb) -> Self {
    Self {
      name: db.name,
      created_at: db.created_at,
      path: db.path,
      kind: db.kind,
      format: db.format,
      size_actual: db.size_actual,
      size_virtual: db.size_virtual,
    }
  }
}
