#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::generic::NetworkKind;

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum ProxyRule {
  /// Redirect http traffic
  Http(ProxyRuleHttp),
  /// Redirect tcp and udp traffic
  Stream(ProxyRuleStream),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ProxySslConfig {
  /// Certificate contents when stored in a TLS secret.
  pub certificate: String,
  /// Private-key contents when stored in a TLS secret.
  pub certificate_key: String,
  /// Certificate-authority contents when stored in a TLS secret.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub certificate_client: Option<String>,
  /// Enable or disable client verification
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub verify_client: Option<bool>,
  /// DH parameter contents when stored in a TLS secret.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub dhparam: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum ProxySsl {
  Config(ProxySslConfig),
  Secret(String),
}

/// Config for targeting a cargo or a vm
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct UpstreamTarget {
  /// The key of the cargo or the vm to target
  pub key: String,
  /// The port of the cargo or the vm to target
  pub port: u16,
  /// The http path to target when using http
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub path: Option<String>,
  /// Disable logging for this target
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub disable_logging: Option<bool>,
  /// SSL configuration for this target
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssl: Option<ProxySsl>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub enum UrlRedirect {
  MovedPermanently,
  Permanent,
  Temporary,
  // TODO: Add other redirect types (https://developer.mozilla.org/en-US/docs/Web/HTTP/Redirections)
  // Found,
  // SeeOther,
}

impl std::fmt::Display for UrlRedirect {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      UrlRedirect::MovedPermanently => write!(f, "301"),
      UrlRedirect::Permanent => write!(f, "308"),
      UrlRedirect::Temporary => write!(f, "307"),
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct HttpTarget {
  /// Url to target
  pub url: String,
  /// Redirect type if it's a redirect
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub redirect: Option<UrlRedirect>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum LocationTarget {
  /// Target an existing cargo
  Upstream(UpstreamTarget),
  /// Target a specific http url
  Http(HttpTarget),
  /// Target a specific unix socket
  Unix(UnixTarget),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct UriTarget {
  /// Uri to target
  pub uri: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct UnixTarget {
  pub unix_path: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum StreamTarget {
  /// Target an existing cargo
  Upstream(UpstreamTarget),
  /// Target a specific uri
  Uri(UriTarget),
  /// Target a specific unix socket
  Unix(UnixTarget),
}

/// Proxy rules modes
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub enum ProxyStreamProtocol {
  Tcp,
  Udp,
}

/// Implement display for ProxyStreamProtocol
/// This is used to display the protocol in the proxy rules config
/// In a human readable format
impl std::fmt::Display for ProxyStreamProtocol {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let data = match self {
      ProxyStreamProtocol::Tcp => "tcp",
      ProxyStreamProtocol::Udp => "udp",
    };
    write!(f, "{data}")
  }
}

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ProxyRuleStream {
  /// Type of the network binding
  pub network: NetworkKind,
  /// Protocol to use Tcp | Udp
  pub protocol: ProxyStreamProtocol,
  /// The port to open on nodes
  pub port: u16,
  /// The ssl configuration
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssl: Option<ProxySsl>,
  /// The target
  pub target: StreamTarget,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct LimitReqZone {
  /// The max size of the cache in megabytes
  pub size: usize,
  /// The max number of request per second
  pub rate: usize,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct LimitReq {
  /// The burst size
  pub burst: usize,
  /// The delay to wait before retrying
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub delay: Option<usize>,
}

/// A validated nginx size value, such as `0`, `10m`, or `2g`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct NginxSize(String);

impl NginxSize {
  fn parse(value: &str) -> Result<Self, String> {
    let (number, suffix) = value.trim().split_at(
      value
        .trim()
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(value.trim().len()),
    );
    if number.is_empty()
      || !suffix.is_empty()
        && !matches!(suffix, "k" | "K" | "m" | "M" | "g" | "G")
    {
      return Err(format!("invalid nginx size: {value}"));
    }
    Ok(Self(value.trim().to_owned()))
  }
}

#[cfg(feature = "serde")]
impl Serialize for NginxSize {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for NginxSize {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    struct SizeVisitor;

    impl de::Visitor<'_> for SizeVisitor {
      type Value = NginxSize;

      fn expecting(
        &self,
        formatter: &mut std::fmt::Formatter,
      ) -> std::fmt::Result {
        formatter.write_str("an nginx size such as 0, 10m, or 2g")
      }

      fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
      where
        E: de::Error,
      {
        NginxSize::parse(&value.to_string()).map_err(E::custom)
      }

      fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
      where
        E: de::Error,
      {
        NginxSize::parse(value).map_err(E::custom)
      }
    }

    deserializer.deserialize_any(SizeVisitor)
  }
}

/// A validated nginx timeout value, such as `900s` or `5m`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct NginxDuration(String);

impl NginxDuration {
  fn parse(value: &str) -> Result<Self, String> {
    let value = value.trim();
    let number_end = value
      .find(|character: char| !character.is_ascii_digit())
      .unwrap_or(value.len());
    let (number, suffix) = value.split_at(number_end);
    if number.is_empty()
      || !matches!(suffix, "ms" | "s" | "m" | "h" | "d" | "w" | "M" | "y")
    {
      return Err(format!("invalid nginx duration: {value}"));
    }
    Ok(Self(value.to_owned()))
  }
}

#[cfg(feature = "serde")]
impl Serialize for NginxDuration {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for NginxDuration {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let value = String::deserialize(deserializer)?;
    Self::parse(&value).map_err(de::Error::custom)
  }
}

/// Supported location-level `proxy_next_upstream` values.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ProxyNextUpstream {
  #[cfg_attr(feature = "serde", serde(rename = "off"))]
  Off,
}

/// A validated nginx proxy cache value: `off` or a configured cache zone.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ProxyCache(String);

impl ProxyCache {
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

#[cfg(feature = "serde")]
impl Serialize for ProxyCache {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for ProxyCache {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    struct CacheVisitor;

    impl de::Visitor<'_> for CacheVisitor {
      type Value = ProxyCache;

      fn expecting(
        &self,
        formatter: &mut std::fmt::Formatter,
      ) -> std::fmt::Result {
        formatter.write_str("false, off, or an nginx cache-zone name")
      }

      fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
      where
        E: de::Error,
      {
        if value {
          Err(E::custom("Cache: true is not a valid nginx cache-zone"))
        } else {
          Ok(ProxyCache("off".to_owned()))
        }
      }

      fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
      where
        E: de::Error,
      {
        if value == "off" {
          return Ok(ProxyCache("off".to_owned()));
        }
        if value.is_empty()
          || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || "_-".contains(character)
          })
        {
          return Err(E::custom("invalid nginx cache-zone name"));
        }
        Ok(ProxyCache(value.to_owned()))
      }
    }

    deserializer.deserialize_any(CacheVisitor)
  }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
  use super::*;

  #[test]
  fn location_options_deserialize_and_validate() {
    let location: ProxyHttpLocation = serde_yaml::from_str(
      "Path: /\nTarget: {Url: https://registry.example}\nClientMaxBodySize: 0\nClientBodyTimeout: 900s\nRequestBuffering: false\nResponseBuffering: false\nCache: false\nReadTimeout: 900s\nSendTimeout: 900s\nConnectTimeout: 5s\nNextUpstream: off\nInterceptErrors: false\n",
    )
    .unwrap();

    assert_eq!(location.client_max_body_size, Some(NginxSize("0".into())));
    assert_eq!(
      location.client_body_timeout,
      Some(NginxDuration("900s".into()))
    );
    assert_eq!(location.request_buffering, Some(false));
    assert_eq!(location.next_upstream, Some(ProxyNextUpstream::Off));
    assert_eq!(location.cache, Some(ProxyCache("off".into())));
    assert!(serde_yaml::from_str::<ProxyHttpLocation>(
      "Path: /\nTarget: {Url: https://registry.example}\nClientBodyTimeout: invalid\n"
    )
    .is_err());
    assert!(
      serde_yaml::from_str::<ProxyHttpLocation>(
        "Path: /\nTarget: {Url: https://registry.example}\nCache: true\n"
      )
      .is_err()
    );
    let location: ProxyHttpLocation = serde_yaml::from_str(
      "Path: /\nTarget: {Url: https://registry.example}\nClientMaxBodySize: 10M\nCache: public_cache\n",
    )
    .unwrap();
    assert_eq!(location.client_max_body_size, Some(NginxSize("10M".into())));
    assert_eq!(location.cache, Some(ProxyCache("public_cache".into())));
  }

  #[test]
  fn existing_location_deserializes_without_options() {
    let location: ProxyHttpLocation = serde_yaml::from_str(
      "Path: /\nTarget: {Url: https://registry.example}\n",
    )
    .unwrap();
    assert!(location.client_max_body_size.is_none());
    assert!(location.next_upstream.is_none());
  }
}

/// Defines a proxy rule location
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ProxyHttpLocation {
  /// The path
  pub path: String,
  /// The target cargo
  pub target: LocationTarget,
  /// Setup limit request for this location
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub limit_req: Option<LimitReq>,
  /// Allowed ip addr
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub allowed_ips: Option<Vec<String>>,
  /// Extras header to add
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub headers: Option<Vec<String>>,
  /// Http version to use
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub version: Option<f64>,
  /// Maximum request body size for this location
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub client_max_body_size: Option<NginxSize>,
  /// Request body read timeout
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub client_body_timeout: Option<NginxDuration>,
  /// Buffer client request bodies
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub request_buffering: Option<bool>,
  /// Buffer upstream responses
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub response_buffering: Option<bool>,
  /// Enable the proxy cache
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cache: Option<ProxyCache>,
  /// Upstream response read timeout
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub read_timeout: Option<NginxDuration>,
  /// Upstream response send timeout
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub send_timeout: Option<NginxDuration>,
  /// Upstream connection timeout
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub connect_timeout: Option<NginxDuration>,
  /// Upstream retry behavior
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub next_upstream: Option<ProxyNextUpstream>,
  /// Intercept upstream errors
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub intercept_errors: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct HstsConfig {
  pub max_age: u64,
  pub preload: bool,
  pub always: bool,
  pub include_sub_domains: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum Hsts {
  Recommended,
  Strict,
  #[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
  Config(HstsConfig),
}

/// Defines a proxy rule http config
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ProxyRuleHttp {
  /// The domain
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub domain: Option<String>,
  /// Port to listen on (default 80 or 443)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub port: Option<u16>,
  /// Hsts configuration
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hsts: Option<Hsts>,
  /// Type of network binding
  pub network: NetworkKind,
  /// Optional limit request zone
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub limit_req_zone: Option<LimitReqZone>,
  /// The locations to handle multiple paths
  pub locations: Vec<ProxyHttpLocation>,
  /// The ssl configuration
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssl: Option<ProxySsl>,
  /// HTTP/3 configuration
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub http3: Option<ProxyHttp3>,
  /// Path to extra config file to include
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub includes: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ProxyHttp3Config {
  /// Enables HTTP/0.9 protocol negotiation used in QUIC interop tests
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hq: Option<bool>,
  /// Maximum concurrent HTTP/3 streams per connection
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub max_concurrent_streams: Option<usize>,
  /// Buffer size for QUIC streams (e.g., "64k", "8k")
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub stream_buffer_size: Option<String>,
  /// QUIC active_connection_id_limit transport parameter
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub active_connection_id_limit: Option<usize>,
  /// Enable routing of QUIC packets using eBPF (Linux 5.7+)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub bpf: Option<bool>,
  /// Enable Generic Segmentation Offload for QUIC (Linux with UDP_SEGMENT)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub gso: Option<bool>,
  /// File path to the QUIC host key
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub host_key: Option<String>,
  /// Enable QUIC address validation (retry)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub retry: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum ProxyHttp3 {
  /// Enables HTTP/3 with default settings
  Bool(bool),
  /// Detailed HTTP/3 configuration
  Config(ProxyHttp3Config),
}

/// Define proxy rules to apply
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceProxyRule {
  /// The rules to apply
  pub rules: Vec<ProxyRule>,
}
