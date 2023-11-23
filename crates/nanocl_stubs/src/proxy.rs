#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum ProxyRule {
  /// Redirect http trafic
  Http(ProxyRuleHttp),
  /// Redirect tcp and udp trafic
  Stream(ProxyRuleStream),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ProxySslConfig {
  /// Path to the certificate
  pub certificate: String,
  /// Path to the certificate key
  pub certificate_key: String,
  /// Path to the certificate client
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
  /// Path to the dhparam file
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub dh_param: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum ProxySsl {
  Config(ProxySslConfig),
  Secret(String),
}

/// Config for targetting a cargo or a vm
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub enum ProxyStreamProtocol {
  Tcp,
  Udp,
}

impl ToString for ProxyStreamProtocol {
  fn to_string(&self) -> String {
    match self {
      ProxyStreamProtocol::Tcp => "tcp",
      ProxyStreamProtocol::Udp => "udp",
    }
    .to_owned()
  }
}

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ProxyRuleStream {
  /// Type of the network binding private | public | internal | namespace:$namespace_name
  pub network: String,
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

/// Defines a proxy rule location
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
}

/// Defines a proxy rule http config
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
  /// Type of private | public | internal | namespace:$namespace_name
  pub network: String,
  /// The locations to handle multiple paths
  pub locations: Vec<ProxyHttpLocation>,
  /// The ssl configuration
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssl: Option<ProxySsl>,
  /// Path to extra config file to include
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub includes: Option<Vec<String>>,
}

/// Define proxy rules to apply
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceProxyRule {
  /// The rules to apply
  pub rules: Vec<ProxyRule>,
}
