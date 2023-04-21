use uuid::Uuid;
use serde::{Serialize, Deserialize, Deserializer};

use crate::schema::http_metrics;

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

fn deserialize_string_to_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<i64>().unwrap_or_default();
  Ok(res)
}

// Not more required for now
fn _deserialize_string_to_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<i32>().unwrap_or_default();
  Ok(res)
}

fn deserialize_string_to_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = String::deserialize(deserializer)?;
  let res = buf.parse::<f64>().unwrap_or_default();
  Ok(res)
}

#[derive(Clone, Debug, Serialize, Deserialize, Insertable)]
#[diesel(table_name = http_metrics)]
pub struct HttpMetricPartial {
  pub date_gmt: chrono::NaiveDateTime,
  pub uri: String,
  pub host: String,
  pub remote_addr: String,
  pub realip_remote_addr: String,
  pub server_protocol: String,
  pub request_method: String,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub content_length: i64,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub status: i64,
  #[serde(deserialize_with = "deserialize_string_to_f64")]
  pub request_time: f64,
  #[serde(deserialize_with = "deserialize_string_to_i64")]
  pub body_bytes_sent: i64,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub proxy_host: Option<String>,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub upstream_addr: Option<String>,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub query_string: Option<String>,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub request_body: Option<String>,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub content_type: Option<String>,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub http_user_agent: Option<String>,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub http_referrer: Option<String>,
  #[serde(deserialize_with = "deserialize_empty_string")]
  pub http_accept_language: Option<String>,
}

#[derive(
  Clone, Debug, Identifiable, Insertable, Queryable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = http_metrics)]
pub struct HttpMetricDbModel {
  pub key: Uuid,
  pub created_at: chrono::NaiveDateTime,
  pub expire_at: chrono::NaiveDateTime,
  pub date_gmt: chrono::NaiveDateTime,
  pub status: i64,
  pub bytes_sent: i64,
  pub content_length: i64,
  pub body_bytes_sent: i64,
  pub request_time: f64,
  pub node_name: String,
  pub uri: String,
  pub host: String,
  pub remote_addr: String,
  pub realip_remote_addr: String,
  pub server_protocol: String,
  pub request_method: String,
  pub proxy_host: Option<String>,
  pub upstream_addr: Option<String>,
  pub query_string: Option<String>,
  pub request_body: Option<String>,
  pub content_type: Option<String>,
  pub http_user_agent: Option<String>,
  pub http_referrer: Option<String>,
  pub http_accept_language: Option<String>,
}
