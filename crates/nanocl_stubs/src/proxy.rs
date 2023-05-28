#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum ProxyRule {
  /// Redirect http trafic
  Http(Vec<ProxyRuleHttp>),
  /// Redirect tcp and udp trafic
  Stream(Vec<ProxyRuleStream>),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxySslConfig {
  /// Path to the certificate
  pub certificate: String,
  /// Path to the certificate key
  pub certificate_key: String,
  /// Path to the certificate client
  pub certificate_client: Option<String>,
  /// Enable or disable client verification
  pub verify_client: Option<bool>,
  /// Path to the dhparam file
  pub dh_param: Option<String>,
}

/// Defines a proxy rule target
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoTarget {
  /// The cargo key
  pub cargo_key: String,
  /// The cargo port
  pub cargo_port: u16,
  /// The http path to target
  pub path: Option<String>,
  /// Disable logging
  pub disable_logging: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum UrlRedirect {
  MovedPermanently,
  Permanent,
  Temporary,
  // TODO ?
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
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct HttpTarget {
  /// Url to target
  pub url: String,
  /// Redirect type if it's a redirect
  pub redirect: Option<UrlRedirect>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum LocationTarget {
  /// Target an existing cargo
  Cargo(CargoTarget),
  /// Target a specific http url
  Http(HttpTarget),
  /// Target a specific unix socket
  Unix(UnixTarget),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct UriTarget {
  /// Uri to target
  pub uri: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct UnixTarget {
  pub unix_path: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged, rename_all = "PascalCase"))]
pub enum StreamTarget {
  /// Target an existing cargo
  Cargo(CargoTarget),
  /// Target a specific uri
  Uri(UriTarget),
  /// Target a specific unix socket
  Unix(UnixTarget),
}

/// Proxy rules modes
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ProxyStreamProtocol {
  Tcp,
  Udp,
}

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRuleStream {
  /// Type of the network binding private | public | internal | namespace:$namespace_name
  pub network: String,
  /// Protocol to use Tcp | Udp
  pub protocol: ProxyStreamProtocol,
  /// The port to open on nodes
  pub port: u16,
  /// The ssl configuration
  pub ssl: Option<ProxySslConfig>,
  /// The target
  pub target: StreamTarget,
}

/// Defines a proxy rule location
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyHttpLocation {
  /// The path
  pub path: String,
  /// The target cargo
  pub target: LocationTarget,
  /// Extras header to add
  pub headers: Option<Vec<String>>,
  /// Http version to use
  pub version: Option<f64>,
}

/// Defines a proxy rule http config
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRuleHttp {
  /// The domain
  pub domain: Option<String>,
  /// Type of private | public | internal | namespace:$namespace_name
  pub network: String,
  /// The locations to handle multiple paths
  pub locations: Vec<ProxyHttpLocation>,
  /// The ssl configuration
  pub ssl: Option<ProxySslConfig>,
  /// Path to extra config file to include
  pub includes: Option<Vec<String>>,
}

/// Define proxy rules to apply
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceProxyRule {
  /// Cargo to watch for changes
  pub watch: Vec<String>,
  /// The rule
  pub rules: ProxyRule,
}
