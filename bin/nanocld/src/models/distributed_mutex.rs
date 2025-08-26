use diesel::prelude::*;

use crate::schema::distributed_mutexes;

#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = distributed_mutexes)]
pub struct DistributedMutexDb {
  pub key: uuid::Uuid,
  pub action: String,
  pub resource_kind: String,
  pub resource_key: String,
  pub node_key: String,
  pub acquired_by: String,
  pub acquired_at: chrono::DateTime<chrono::Utc>,
  pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug)]
pub struct DistributedMutexPartial {
  pub action: String,
  pub resource_kind: String,
  pub resource_key: String,
  pub node_key: String,
  pub acquired_by: String,
}

impl From<DistributedMutexPartial> for DistributedMutexDb {
  fn from(value: DistributedMutexPartial) -> Self {
    Self {
      key: uuid::Uuid::new_v4(),
      action: value.action,
      resource_kind: value.resource_kind,
      resource_key: value.resource_key,
      node_key: value.node_key,
      acquired_by: value.acquired_by,
      acquired_at: chrono::Utc::now(),
      expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
    }
  }
}
