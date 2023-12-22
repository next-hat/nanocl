use diesel::prelude::*;

use crate::schema::specs;

/// This structure represent the specification of an object. (job, cargo, vm, ...)
/// We store it with it's version to ensure backward compatibility.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(table_name = specs)]
#[diesel(primary_key(key))]
pub struct SpecDb {
  /// The related resource kind reference
  pub key: uuid::Uuid,
  /// When the resource kind version have been created
  pub created_at: chrono::NaiveDateTime,
  /// Kind of kind key
  pub kind_name: String,
  /// Relation to the kind object
  pub kind_key: String,
  /// Version of the resource kind
  pub version: String,
  /// Config of the resource kind version
  pub data: serde_json::Value,
  /// Metadata (user defined) of the resource kind version
  pub metadata: Option<serde_json::Value>,
}
