use diesel::prelude::*;

use crate::schema::{resource_kinds, resource_kind_versions};

#[derive(Clone, Debug)]
pub struct ResourceKindPartial {
  pub(crate) name: String,
  pub(crate) version: String,
  pub(crate) schema: serde_json::Value,
}

#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(resource_kind_name, version))]
#[diesel(table_name = resource_kind_versions)]
pub struct ResourceKindVersionDbModel {
  pub(crate) resource_kind_name: String,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) version: String,
  pub(crate) schema: serde_json::Value,
}

#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(name))]
#[diesel(table_name = resource_kinds)]
pub struct ResourceKindDbModel {
  pub(crate) name: String,
  pub(crate) created_at: chrono::NaiveDateTime,
}
