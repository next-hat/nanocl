use uuid::Uuid;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct HttpMetric {
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
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub proxy_host: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub upstream_addr: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub query_string: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub request_body: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub content_type: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub http_user_agent: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub http_referrer: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub http_accept_language: Option<String>,
}
