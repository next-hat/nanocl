use uuid::Uuid;
use tokio::task::JoinHandle;
use chrono::{DateTime, FixedOffset};
use serde::{Serialize, Deserialize, Deserializer};

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::GenericFilter;

use crate::schema::{http_metrics, stream_metrics};

use super::{Pool, Repository};
use super::generic::ToMeticDb;

/// Serde helper to deserialize string that can be empty to `Option<String>`.
fn deserialize_empty_string<'de, D>(
  deserializer: D,
) -> Result<Option<String>, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  if buf.is_empty() {
    Ok(None)
  } else {
    Ok(Some(buf))
  }
}

/// Serde helper to deserialize string to `i64`.
fn deserialize_string_to_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<i64>().unwrap_or_default();
  Ok(res)
}

/// Serde helper to deserialize string to `f64`.
fn deserialize_string_to_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<f64>().unwrap_or_default();
  Ok(res)
}

/// This structure represent a partial http metric.
/// It is used to insert http metrics in the database.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "PascalCase"))]
pub struct HttpMetricPartial {
  /// The date gmt of the metric
  pub date_gmt: DateTime<FixedOffset>,
  /// The target uri of the request
  pub uri: String,
  /// The target host of the request
  pub host: String,
  /// The remote address of the request
  pub remote_addr: String,
  /// The real ip remote address of the request
  pub realip_remote_addr: String,
  /// The server protocol of the request
  pub server_protocol: String,
  /// The method of the request
  pub request_method: String,
  /// The bytes sent of the request
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub bytes_sent: i64,
  /// The content length of the request
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub content_length: i64,
  /// The status of the request
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub status: i64,
  /// The time of the request
  #[serde(deserialize_with = "deserialize_string_to_f64")]
  pub request_time: f64,
  /// The number of bytes send in the body of the request
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub body_bytes_sent: i64,
  /// The proxy host of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub proxy_host: Option<String>,
  /// The upstream address of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub upstream_addr: Option<String>,
  /// The query string of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub query_string: Option<String>,
  /// The body of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub request_body: Option<String>,
  /// The content type of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub content_type: Option<String>,
  /// The http user agent of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub http_user_agent: Option<String>,
  /// The http referrer of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub http_referrer: Option<String>,
  /// The http accept language of the request
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub http_accept_language: Option<String>,
}

impl ToMeticDb for HttpMetricPartial {
  type MetricDb = HttpMetricDb;

  fn to_metric_db(self, node_name: &str) -> Self::MetricDb {
    HttpMetricDb {
      key: Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      expire_at: chrono::Utc::now().naive_utc() + chrono::Duration::days(30),
      bytes_sent: self.bytes_sent,
      date_gmt: self.date_gmt.naive_utc(),
      uri: self.uri,
      host: self.host,
      remote_addr: self.remote_addr,
      node_name: node_name.to_owned(),
      realip_remote_addr: self.realip_remote_addr,
      server_protocol: self.server_protocol,
      request_method: self.request_method,
      content_length: self.content_length,
      status: self.status,
      request_time: self.request_time,
      body_bytes_sent: self.body_bytes_sent,
      proxy_host: self.proxy_host,
      upstream_addr: self.upstream_addr,
      query_string: self.query_string,
      request_body: self.request_body,
      content_type: self.content_type,
      http_user_agent: self.http_user_agent,
      http_referrer: self.http_referrer,
      http_accept_language: self.http_accept_language,
    }
  }
}

/// ## HttpMetricDb
///
/// This structure represent a http metric in the database.
/// A http metric is a data point that can be used to monitor the health of a service.
/// We use the `node_name` to link the metric to the node that handled the request.
///
#[derive(
  Clone, Debug, Identifiable, Insertable, Queryable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = http_metrics)]
#[serde(rename_all = "PascalCase")]
pub struct HttpMetricDb {
  /// The key of the metric in the database `UUID`
  pub key: Uuid,
  /// When the metric was created
  pub created_at: chrono::NaiveDateTime,
  /// When the metric will expire
  pub expire_at: chrono::NaiveDateTime,
  /// The date gmt of the metric
  pub date_gmt: chrono::NaiveDateTime,
  /// The status of the request
  pub status: i64,
  /// The bytes sent of the request
  pub bytes_sent: i64,
  /// The content length of the request
  pub content_length: i64,
  /// The number of bytes send in the body of the request
  pub body_bytes_sent: i64,
  /// The time of the request
  pub request_time: f64,
  /// The node that handled the request
  pub node_name: String,
  /// The target uri of the request
  pub uri: String,
  /// The target host of the request
  pub host: String,
  /// The remote address of the request
  pub remote_addr: String,
  /// The real ip remote address of the request
  pub realip_remote_addr: String,
  /// The server protocol of the request
  pub server_protocol: String,
  /// The method of the request
  pub request_method: String,
  /// The proxy host of the request
  pub proxy_host: Option<String>,
  /// The upstream address of the request
  pub upstream_addr: Option<String>,
  /// The query string of the request
  pub query_string: Option<String>,
  /// The body of the request
  pub request_body: Option<String>,
  /// The content type of the request
  pub content_type: Option<String>,
  /// The http user agent of the request
  pub http_user_agent: Option<String>,
  /// The http referrer of the request
  pub http_referrer: Option<String>,
  /// The http accept language of the request
  pub http_accept_language: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "PascalCase"))]
pub struct StreamMetricPartial {
  pub date_gmt: DateTime<FixedOffset>,
  pub remote_addr: String,
  pub upstream_addr: String,
  pub protocol: Option<String>,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub status: i64,
  pub session_time: String,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub bytes_sent: i64,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub bytes_received: i64,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub upstream_bytes_sent: i64,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub upstream_bytes_received: i64,
  pub upstream_connect_time: String,
}

#[derive(
  Clone, Debug, Identifiable, Insertable, Queryable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = stream_metrics)]
#[serde(rename_all = "PascalCase")]
pub struct StreamMetricDb {
  /// The key of the metric in the database `UUID`
  pub key: Uuid,
  /// When the metric was created
  pub created_at: chrono::NaiveDateTime,
  /// When the metric will expire
  pub expire_at: chrono::NaiveDateTime,
  /// The date gmt of the metric
  pub date_gmt: chrono::NaiveDateTime,
  /// The remote address of the request
  pub remote_addr: String,
  /// The upstream address of the request
  pub upstream_addr: String,
  /// The protocol of the request
  pub protocol: Option<String>,
  /// The status of the request
  pub status: i64,
  /// The session time of the request
  pub session_time: String,
  /// The bytes sent of the request
  pub bytes_sent: i64,
  /// The bytes received of the request
  pub bytes_received: i64,
  /// Number of bytes sent to upstream request
  pub upstream_bytes_sent: i64,
  /// Number of bytes received from upstream response
  pub upstream_bytes_received: i64,
  /// Time to connect to upstream
  pub upstream_connect_time: String,
  /// The node that handled the request
  pub node_name: String,
}

impl ToMeticDb for StreamMetricPartial {
  type MetricDb = StreamMetricDb;

  fn to_metric_db(self, node_name: &str) -> Self::MetricDb {
    Self::MetricDb {
      key: Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      expire_at: chrono::Utc::now().naive_utc() + chrono::Duration::days(30),
      date_gmt: self.date_gmt.naive_utc(),
      remote_addr: self.remote_addr,
      upstream_addr: self.upstream_addr,
      protocol: self.protocol,
      status: self.status,
      session_time: self.session_time,
      bytes_sent: self.bytes_sent,
      bytes_received: self.bytes_received,
      upstream_bytes_sent: self.upstream_bytes_sent,
      upstream_bytes_received: self.upstream_bytes_received,
      upstream_connect_time: self.upstream_connect_time,
      node_name: node_name.to_owned(),
    }
  }
}

impl Repository for HttpMetricDb {
  type Table = http_metrics::table;
  type Item = HttpMetricDb;
  type UpdateItem = HttpMetricDb;

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
  }
}

impl Repository for StreamMetricDb {
  type Table = stream_metrics::table;
  type Item = StreamMetricDb;
  type UpdateItem = StreamMetricDb;

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
  }
}
