use diesel::prelude::*;

use serde::{Serialize, Deserialize};

use crate::schema::vm_images;

#[derive(
  Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = vm_images)]
pub struct VmImageDbModel {
  pub(crate) name: String,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) path: String,
  pub(crate) kind: String,
  pub(crate) size: i64,
  pub(crate) checksum: String,
}
