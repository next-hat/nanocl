use uuid::Uuid;
use chrono::{DateTime, FixedOffset};
use serde::{Serialize, Deserialize, Deserializer};

use crate::schema::http_metrics;

/// ## deserialize empty string
///
/// Serde helper to deserialize string that can be empty to `Option<String>`.
///
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

/// ## deserialize string to i64
///
/// Serde helper to deserialize string to `i64`.
///
fn deserialize_string_to_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<i64>().unwrap_or_default();
  Ok(res)
}

/// ## deserialize string to f64
///
/// Serde helper to deserialize string to `f64`.
fn deserialize_string_to_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<f64>().unwrap_or_default();
  Ok(res)
}

/// ## HttpMetricPartial
///
/// This structure represent a partial http metric.
/// It is used to insert http metrics in the database.
///
#[derive(Clone, Debug, Serialize, Deserialize, Insertable)]
#[serde(rename_all(serialize = "PascalCase"))]
#[diesel(table_name = http_metrics)]
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

impl HttpMetricPartial {
  pub(crate) fn to_db_model(&self, node_name: &str) -> HttpMetricDbModel {
    HttpMetricDbModel {
      key: Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      expire_at: chrono::Utc::now().naive_utc() + chrono::Duration::days(30),
      bytes_sent: self.bytes_sent,
      date_gmt: self.date_gmt.naive_utc(),
      uri: self.uri.clone(),
      host: self.host.clone(),
      remote_addr: self.remote_addr.clone(),
      node_name: node_name.to_string(),
      realip_remote_addr: self.realip_remote_addr.clone(),
      server_protocol: self.server_protocol.clone(),
      request_method: self.request_method.clone(),
      content_length: self.content_length,
      status: self.status,
      request_time: self.request_time,
      body_bytes_sent: self.body_bytes_sent,
      proxy_host: self.proxy_host.clone(),
      upstream_addr: self.upstream_addr.clone(),
      query_string: self.query_string.clone(),
      request_body: self.request_body.clone(),
      content_type: self.content_type.clone(),
      http_user_agent: self.http_user_agent.clone(),
      http_referrer: self.http_referrer.clone(),
      http_accept_language: self.http_accept_language.clone(),
    }
  }
}

/// ## HttpMetricDbModel
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
pub struct HttpMetricDbModel {
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

// TODO: implement Stream Metrics support for tcp/udp protocols
// #[derive(Clone, Debug, Serialize, Deserialize)]
// #[serde(rename_all(serialize = "PascalCase"))]
// pub struct StreamMetricPartial {
//   pub date_gmt: DateTime<FixedOffset>,
//   pub remote_addr: String,
//   pub upstream_addr: String,
//   pub protocol: String,
//   pub status: i64,
//   pub session_time: String,
//   pub bytes_sent: String,
//   pub bytes_received: String,
// }
