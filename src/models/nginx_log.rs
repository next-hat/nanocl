use chrono::{DateTime, FixedOffset};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct NginxLogItem {
  pub(crate) key: String,
  pub(crate) date_gmt: DateTime<FixedOffset>,
  pub(crate) uri: String,
  pub(crate) host: String,
  pub(crate) remote_addr: String,
  pub(crate) realip_remote_addr: String,
  pub(crate) server_protocol: String,
  pub(crate) request_method: String,
  pub(crate) content_length: i64,
  pub(crate) status: i32,
  pub(crate) request_time: f64,
  pub(crate) body_bytes_sent: i64,
  pub(crate) proxy_host: Option<String>,
  pub(crate) upstream_addr: Option<String>,
  pub(crate) query_string: Option<String>,
  pub(crate) request_body: Option<String>,
  pub(crate) content_type: Option<String>,
  pub(crate) http_user_agent: Option<String>,
  pub(crate) http_referrer: Option<String>,
  pub(crate) http_accept_language: Option<String>,
}
