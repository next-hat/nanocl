use diesel::prelude::*;

use crate::schema::{resource_kinds, resource_kind_versions};

/// ## ResourceKindVersionDbModel
///
/// This structure represent the resource kind verion in the database.
/// A resource kind version represent the version of a resource kind.
/// It is stored as a json object in the database.
/// We use the `resource_kind_name` to link to the resource kind.
///
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(resource_kind_name, version))]
#[diesel(table_name = resource_kind_versions)]
pub struct ResourceKindVersionDbModel {
  pub(crate) resource_kind_name: String,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) version: String,
  pub(crate) schema: Option<serde_json::Value>,
  pub(crate) url: Option<String>,
}

/// ## ResourceKindDbModel
///
/// This structure represent the resource kind in the database.
/// A resource kind represent the kind of a resource.
/// It is stored with a version that containt the schema or and url of a service to call.
///
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(name))]
#[diesel(table_name = resource_kinds)]
pub struct ResourceKindDbModel {
  pub(crate) name: String,
  pub(crate) created_at: chrono::NaiveDateTime,
}

/// ## ResourceKindPartial
///
/// This structure is a partial representation of a resource kind.
/// It is used to create a resource kind in the database.
///
#[derive(Clone, Debug)]
pub struct ResourceKindPartial {
  /// The name of the resource kind
  pub(crate) name: String,
  /// The version of the resource kind
  pub(crate) version: String,
  /// The JSONSchema of the resource of this kind and version
  pub(crate) schema: Option<serde_json::Value>,
  /// The service to call when creating, updating or deleting a resource of this kind and version
  pub(crate) url: Option<String>,
}
