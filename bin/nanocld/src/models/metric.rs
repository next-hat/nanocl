use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::schema::metrics;

#[derive(Debug, Identifiable, Queryable, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricDbModel {
  pub key: Uuid,
  pub created_at: chrono::NaiveDateTime,
  pub expire_at: chrono::NaiveDateTime,
  pub node_name: String,
  pub kind: String,
  pub data: serde_json::Value,
}

#[derive(Clone, Debug, Default, Insertable)]
#[diesel(table_name = metrics)]
pub struct MetricInsertDbModel {
  pub kind: String,
  pub node_name: String,
  pub data: serde_json::Value,
}
