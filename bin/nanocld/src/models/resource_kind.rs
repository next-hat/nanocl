use diesel::prelude::*;

use nanocl_error::io::{IoError, FromIo};

use nanocl_stubs::resource_kind::{
  ResourceKind, ResourceKindPartial, ResourceKindVersion,
};

use crate::{
  schema::{resource_kinds, resource_kind_versions},
  utils,
};

/// This structure represent the resource kind verion in the database.
/// A resource kind version represent the version of a resource kind.
/// It is stored as a json object in the database.
/// We use the `resource_kind_name` to link to the resource kind.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(table_name = resource_kind_versions)]
#[diesel(primary_key(key))]
pub struct ResourceKindVersionDb {
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

/// This structure represent the resource kind in the database.
/// A resource kind represent the kind of a resource.
/// It is stored with a version that containt the schema or and url of a service to call.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(name))]
#[diesel(table_name = resource_kinds)]
pub struct ResourceKindDb {
  /// Name of the kind
  pub name: String,
  /// Last version
  pub version_key: uuid::Uuid,
  /// When the kind have been created
  pub created_at: chrono::NaiveDateTime,
}

#[derive(Clone, Debug, AsChangeset)]
#[diesel(table_name = resource_kinds)]
pub struct ResourceKindDbUpdate {
  pub version_key: uuid::Uuid,
}

impl TryFrom<ResourceKindVersionDb> for ResourceKind {
  type Error = IoError;

  fn try_from(db: ResourceKindVersionDb) -> Result<Self, Self::Error> {
    let data = serde_json::from_value(db.data)
      .map_err(|err| err.map_err_context(|| "ResourceKind"))?;
    Ok(Self {
      name: db.kind_key,
      version: db.version,
      created_at: db.created_at,
      metadata: db.metadata,
      data,
    })
  }
}

impl TryFrom<ResourceKindVersionDb> for ResourceKindVersion {
  type Error = IoError;

  fn try_from(db: ResourceKindVersionDb) -> Result<Self, Self::Error> {
    let data = serde_json::from_value(db.data)
      .map_err(|err| err.map_err_context(|| "ResourceKind"))?;
    Ok(Self {
      key: db.key,
      created_at: db.created_at,
      kind_key: db.kind_key,
      version: db.version,
      metadata: db.metadata,
      data,
    })
  }
}

impl TryFrom<&ResourceKindPartial> for ResourceKindVersionDb {
  type Error = IoError;

  fn try_from(p: &ResourceKindPartial) -> Result<Self, Self::Error> {
    let data = serde_json::to_value(&p.data)
      .map_err(|err| err.map_err_context(|| "ResourceKind"))?;
    utils::key::ensure_kind(&p.name)?;
    if p.data.url.is_none() && p.data.schema.is_none() {
      return Err(IoError::invalid_input(
        "ResourceKind",
        "Invalid data nor url or schema defined",
      ));
    }
    Ok(ResourceKindVersionDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: "ResourceKind".to_owned(),
      kind_key: p.name.clone(),
      metadata: p.metadata.clone(),
      version: p.version.clone(),
      data,
    })
  }
}
