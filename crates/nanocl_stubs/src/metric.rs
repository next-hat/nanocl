use chrono::{DateTime, FixedOffset};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Metric entry
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Metric {
  /// The key of the metric in the database `UUID`
  pub key: uuid::Uuid,
  /// When the metric was created
  pub created_at: chrono::NaiveDateTime,
  /// When the metric will expire
  pub expires_at: chrono::NaiveDateTime,
  /// The node where the metric come from
  pub node_name: String,
  /// The kind of the metric
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
  /// Optional note about the metric
  pub note: Option<String>,
}

/// Used to create a new metric
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct MetricPartial {
  /// The kind of the metric
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
  /// Optional note about the metric
  pub note: Option<String>,
}

/// ## deserialize empty string
///
/// Serde helper to deserialize string that can be empty to `Option<String>`.
///
#[cfg(feature = "serde")]
fn deserialize_empty_string<'de, D>(
  deserializer: D,
) -> Result<Option<String>, D::Error>
where
  D: serde::Deserializer<'de>,
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
#[cfg(feature = "serde")]
fn deserialize_string_to_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
  D: serde::Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<i64>().unwrap_or_default();
  Ok(res)
}

/// ## deserialize string to f64
///
/// Serde helper to deserialize string to `f64`.
#[cfg(feature = "serde")]
fn deserialize_string_to_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
  D: serde::Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<f64>().unwrap_or_default();
  Ok(res)
}

/// Represent a http metric for a metric kind ncproxy.io/http
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all(serialize = "PascalCase")))]
pub struct HttpMetric {
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
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub bytes_sent: i64,
  /// The content length of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub content_length: i64,
  /// The status of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub status: i64,
  /// The time of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_f64")
  )]
  pub request_time: f64,
  /// The number of bytes send in the body of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub body_bytes_sent: i64,
  /// The proxy host of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub proxy_host: Option<String>,
  /// The upstream address of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub upstream_addr: Option<String>,
  /// The query string of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub query_string: Option<String>,
  /// The body of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub request_body: Option<String>,
  /// The content type of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub content_type: Option<String>,
  /// The http user agent of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub http_user_agent: Option<String>,
  /// The http referrer of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub http_referrer: Option<String>,
  /// The http accept language of the request
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_empty_string")
  )]
  pub http_accept_language: Option<String>,
}

/// Represent a stream metric for a metric kind ncproxy.io/stream
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all(serialize = "PascalCase")))]
pub struct StreamMetric {
  pub date_gmt: DateTime<FixedOffset>,
  pub remote_addr: String,
  pub upstream_addr: String,
  pub protocol: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub status: i64,
  pub session_time: String,
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub bytes_sent: i64,
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub bytes_received: i64,
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub upstream_bytes_sent: i64,
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "deserialize_string_to_i64")
  )]
  pub upstream_bytes_received: i64,
  pub upstream_connect_time: String,
}
