use serde::{Serialize, Deserialize};

use crate::schema::metrics;

#[derive(Debug, Identifiable, Queryable, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricDbModel {
  pub(crate) key: uuid::Uuid,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) expire_at: chrono::NaiveDateTime,
  pub(crate) node_name: String,
  pub(crate) kind: String,
  pub(crate) data: serde_json::Value,
}

#[derive(Clone, Debug, Default, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricInsertDbModel {
  pub(crate) kind: String,
  pub(crate) node_name: String,
  pub(crate) data: serde_json::Value,
}
