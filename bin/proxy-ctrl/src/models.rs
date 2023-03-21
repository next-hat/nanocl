use serde::{Serialize, Deserialize};

#[cfg(feature = "bschemars")]
use schemars::JsonSchema;

/// Proxy rules modes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub enum ProxyRule {
  /// Redirect http trafic
  Http(ProxyRuleHttp),
  /// Redirect tcp and udp trafic
  Stream(ProxyRuleStream),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub struct ProxySslConfig {
  /// Path to the certificate
  pub certificate: String,
  /// Path to the certificate key
  pub certificate_key: String,
  /// Path to the dhparam file
  pub dh_param: Option<String>,
}

/// Defines a proxy rule target
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub struct CargoTarget {
  /// The cargo key
  pub key: String,
  /// The cargo port
  pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub enum UrlRedirect {
  MovedPermanently,
  PermanentRedirect,
  TemporaryRedirect,
  // TODO ?
  // Found,
  // SeeOther,
}

impl std::fmt::Display for UrlRedirect {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      UrlRedirect::MovedPermanently => write!(f, "301"),
      UrlRedirect::PermanentRedirect => write!(f, "308"),
      UrlRedirect::TemporaryRedirect => write!(f, "307"),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub struct HttpTarget {
  pub url: String,
  pub redirect: Option<UrlRedirect>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub enum LocationTarget {
  Cargo(CargoTarget),
  Http(HttpTarget),
}

/// Proxy rules modes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub enum ProxyStreamProtocol {
  Tcp,
  Udp,
}

/// Proxy rules modes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub struct ProxyRuleStream {
  /// Type of the network binding private | public | internal | namespace:$namespace_name
  pub network: String,
  /// Protocol to use Tcp | Udp
  pub protocol: ProxyStreamProtocol,
  /// The port to open on nodes
  pub port: u16,
  /// The ssl configuration
  pub ssl: Option<ProxySslConfig>,
  /// The target cargo
  pub target: CargoTarget,
}

/// Defines a proxy rule location
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub struct ProxyHttpLocation {
  /// The path
  pub path: String,
  /// The target cargo
  pub target: LocationTarget,
}

/// Defines a proxy rule http config
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bschemars", derive(JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub struct ResourceProxyRule {
  /// Cargo to watch for changes
  pub watch: Vec<String>,
  /// The rule
  pub rule: ProxyRule,
}
