use serde::Deserializer;
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
  pub proxy_host: Option<String>,
  pub upstream_addr: Option<String>,
  pub query_string: Option<String>,
  pub request_body: Option<String>,
  pub content_type: Option<String>,
  pub http_user_agent: Option<String>,
  pub http_referrer: Option<String>,
  pub http_accept_language: Option<String>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct HttpMetricListQuery {
  pub limit: Option<i64>,
  pub offset: Option<i64>,
}

#[cfg(feature = "serde")]
fn deserialize_status_between<'de, D>(
  deserializer: D,
) -> Result<Option<(i64, Option<i64>)>, D::Error>
where
  D: Deserializer<'de>,
{
  let buf = Option::<String>::deserialize(deserializer)?;
  let buf = match &buf {
    None => {
      return Ok(None);
    }
    Some(buf) => buf,
  };
  let res: Vec<_> = buf
    .split(',')
    .map(|s| s.parse::<i64>().unwrap_or_default())
    .collect();

  let status_min = *res.first().unwrap_or(&0);
  let status_max = res.get(1).copied();

  Ok(Some((status_min, status_max)))
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct HttpMetricCountQuery {
  #[cfg_attr(
    feature = "serde",
    serde(default, deserialize_with = "deserialize_status_between")
  )]
  pub status: Option<(i64, Option<i64>)>,
}
