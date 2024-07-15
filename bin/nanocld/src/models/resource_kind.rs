use diesel::prelude::*;

use nanocl_error::io::{FromIo, IoError};

use nanocl_stubs::resource_kind::{
  ResourceKind, ResourceKindPartial, ResourceKindVersion,
};

use crate::{schema::resource_kinds, utils};

use super::SpecDb;

/// This structure represent the resource kind in the database.
/// A resource kind represent the kind of a resource.
/// It is stored with a version that containt the schema or and url of a service to call.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(name))]
#[diesel(table_name = resource_kinds)]
pub struct ResourceKindDb {
  /// Name of the kind
  pub name: String,
  /// When the kind have been created
  pub created_at: chrono::NaiveDateTime,
  /// Last version
  pub spec_key: uuid::Uuid,
}

#[derive(Clone, Debug, AsChangeset)]
#[diesel(table_name = resource_kinds)]
pub struct ResourceKindDbUpdate {
  pub spec_key: uuid::Uuid,
}

impl TryFrom<SpecDb> for ResourceKind {
  type Error = IoError;

  fn try_from(db: SpecDb) -> Result<Self, Self::Error> {
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

impl TryFrom<SpecDb> for ResourceKindVersion {
  type Error = IoError;

  fn try_from(db: SpecDb) -> Result<Self, Self::Error> {
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

impl TryFrom<&ResourceKindPartial> for SpecDb {
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
    Ok(SpecDb {
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
