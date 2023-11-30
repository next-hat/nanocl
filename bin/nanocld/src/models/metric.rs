use std::sync::Arc;

use uuid::Uuid;
use diesel::prelude::*;
use tokio::task::JoinHandle;
use serde::{Serialize, Deserialize};

use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocl_stubs::generic::GenericFilter;

use crate::{schema::metrics, utils};

use super::{Pool, Repository};

/// This structure represent a metric in the database.
/// A metric is a data point that can be used to monitor the system.
/// It is stored as a json object in the database.
/// We use the `node_name` to link the metric to the node.
#[derive(
  Debug, Insertable, Identifiable, Queryable, Serialize, Deserialize,
)]
#[serde(rename_all = "PascalCase")]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricDb {
  /// The key of the metric in the database `UUID`
  pub key: Uuid,
  /// When the metric was created
  pub created_at: chrono::NaiveDateTime,
  /// When the metric will expire
  pub expire_at: chrono::NaiveDateTime,
  /// The node where the metric come from
  pub node_name: String,
  /// The kind of the metric (CPU, MEMORY, DISK, NETWORK)
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
}

/// This structure is used to insert a metric in the database.
#[derive(Clone, Debug)]
pub struct MetricPartial {
  /// The kind of the metric (CPU, MEMORY, DISK, NETWORK)
  pub kind: String,
  /// The node where the metric come from
  pub node_name: String,
  /// The data of the metric
  pub data: serde_json::Value,
}

impl From<&MetricPartial> for MetricDb {
  fn from(p: &MetricPartial) -> Self {
    MetricDb {
      key: Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      expire_at: chrono::Utc::now().naive_utc(),
      node_name: p.node_name.clone(),
      kind: p.kind.clone(),
      data: p.data.clone(),
    }
  }
}

impl Repository for MetricDb {
  type Table = metrics::table;
  type Item = MetricDb;
  type UpdateItem = MetricDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    let mut query = metrics::dsl::metrics
      .order(metrics::dsl::created_at.desc())
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    // if let Some(value) = r#where.get("Key") {
    //   gen_where4string!(query, stream_metrics::dsl::key, value);
    // }
    // if let Some(value) = r#where.get("Name") {
    //   gen_where4string!(query, stream_metrics::dsl::name, value);
    // }
    // if let Some(value) = r#where.get("Kind") {
    //   gen_where4string!(query, stream_metrics::dsl::kind, value);
    // }
    // if let Some(value) = r#where.get("NodeId") {
    //   gen_where4string!(query, stream_metrics::dsl::node_id, value);
    // }
    // if let Some(value) = r#where.get("KindId") {
    //   gen_where4string!(query, stream_metrics::dsl::kind_id, value);
    // }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    let mut query = metrics::dsl::metrics
      .order(metrics::dsl::created_at.desc())
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    // if let Some(value) = r#where.get("Key") {
    //   gen_where4string!(query, stream_metrics::dsl::key, value);
    // }
    // if let Some(value) = r#where.get("Name") {
    //   gen_where4string!(query, stream_metrics::dsl::name, value);
    // }
    // if let Some(value) = r#where.get("Kind") {
    //   gen_where4string!(query, stream_metrics::dsl::kind, value);
    // }
    // if let Some(value) = r#where.get("NodeId") {
    //   gen_where4string!(query, stream_metrics::dsl::node_id, value);
    // }
    // if let Some(value) = r#where.get("KindId") {
    //   gen_where4string!(query, stream_metrics::dsl::kind_id, value);
    // }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      Ok::<_, IoError>(items)
    })
  }
}
