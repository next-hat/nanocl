use std::sync::Arc;

use diesel::prelude::*;
use ntex::rt::JoinHandle;
use serde::{Serialize, Deserialize};

use nanocl_error::io::{IoResult, IoError};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  vm_image::VmImage,
};

use crate::{utils, gen_where4string, schema::vm_images};

use super::{Pool, Repository};

/// This structure represent a virtual machine image in the database.
/// A virtual machine image is a file that represent a virtual machine disk.
///
/// Two kind of virtual machine image are supported:
/// - Base: A base image is a virtual machine image that is not based on another image.
/// - Snapshot: A snapshot image is a virtual machine image that is based on a base image.
///
/// A `Snapshot` of a `Base` image will alway be use to create a virtual machine.
#[derive(
  Clone, Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = vm_images)]
#[serde(rename_all = "PascalCase")]
pub struct VmImageDb {
  /// The name of the virtual machine image
  pub name: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The kind of the virtual machine image (Base, Snapshot)
  pub kind: String,
  /// The path of the virtual machine image
  pub path: String,
  /// The format of the virtual machine image
  pub format: String,
  /// The actual size of the virtual machine image
  pub size_actual: i64,
  /// The virtual size of the virtual machine image
  pub size_virtual: i64,
  /// The parent of the virtual machine image
  pub parent: Option<String>,
}

/// This structure is used to update a virtual machine image in the database.
#[derive(Clone, Debug, AsChangeset)]
#[diesel(table_name = vm_images)]
pub struct VmImageUpdateDb {
  /// The actual size of the virtual machine image
  pub size_actual: i64,
  /// The virtual size of the virtual machine image
  pub size_virtual: i64,
}

/// This structure is used to parse the output of the qemu-img info command.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct QemuImgInfo {
  /// The format of the virtual machine image
  pub format: String,
  /// The virtual size of the virtual machine image
  pub virtual_size: i64,
  /// The actual size of the virtual machine image
  pub actual_size: i64,
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

impl Repository for VmImageDb {
  type Table = vm_images::table;
  type Item = VmImageDb;
  type UpdateItem = VmImageUpdateDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::trace!("VmImageDb::find_one: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = vm_images::dsl::vm_images.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vm_images::dsl::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, vm_images::dsl::kind, value);
    }
    if let Some(value) = r#where.get("parent") {
      gen_where4string!(query, vm_images::dsl::parent, value);
    }
    if let Some(value) = r#where.get("format") {
      gen_where4string!(query, vm_images::dsl::format, value);
    }
    if let Some(value) = r#where.get("path") {
      gen_where4string!(query, vm_images::dsl::path, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::trace!("VmImageDb::find: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = vm_images::dsl::vm_images.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vm_images::dsl::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, vm_images::dsl::kind, value);
    }
    if let Some(value) = r#where.get("parent") {
      gen_where4string!(query, vm_images::dsl::parent, value);
    }
    if let Some(value) = r#where.get("format") {
      gen_where4string!(query, vm_images::dsl::format, value);
    }
    if let Some(value) = r#where.get("path") {
      gen_where4string!(query, vm_images::dsl::path, value);
    }
    let limit = filter.limit.unwrap_or(100);
    query = query.limit(limit as i64);
    if let Some(offset) = filter.offset {
      query = query.offset(offset as i64);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(items)
    })
  }
}

impl VmImageDb {
  pub(crate) async fn find_by_parent(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<VmImageDb>> {
    let filter = GenericFilter::new()
      .r#where("parent", GenericClause::Eq(name.to_owned()));
    VmImageDb::find(&filter, pool).await?
  }
}
