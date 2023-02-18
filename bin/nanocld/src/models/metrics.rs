use crate::schema::metrics;

#[derive(Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricDbModel {
  pub(crate) key: uuid::Uuid,
  pub(crate) kind: String,
  pub(crate) data: serde_json::Value,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) expire_at: chrono::NaiveDateTime,
}

#[derive(Debug, Default, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricInsertDbModel {
  pub(crate) kind: String,
  pub(crate) data: serde_json::Value,
}
