#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Resource kinds
/// It is used to define the kind of a resource
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ResourceKind {
  ProxyRule,
  Unknown,
}

impl std::fmt::Display for ResourceKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ResourceKind::ProxyRule => write!(f, "ProxyRule"),
      ResourceKind::Unknown => write!(f, "Unknown"),
    }
  }
}

impl From<String> for ResourceKind {
  fn from(kind: String) -> Self {
    match kind.as_str() {
      "ProxyRule" => ResourceKind::ProxyRule,
      _ => ResourceKind::Unknown,
    }
  }
}

impl From<ResourceKind> for String {
  fn from(kind: ResourceKind) -> Self {
    match kind {
      ResourceKind::ProxyRule => "ProxyRule".into(),
      ResourceKind::Unknown => "Unknown".into(),
    }
  }
}

/// Resource partial is a payload used to create a new resource
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourcePartial {
  /// The name of the resource
  pub name: String,
  /// The kind of the resource
  pub kind: ResourceKind,
  /// The config of the resource
  pub config: serde_json::Value,
}

/// Resource is a configuration with a name and a kind
/// It is used to define [proxy rules](ProxyRule) and other kind of config
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Resource {
  /// The name of the resource
  pub name: String,
  /// The kind of the resource
  pub kind: ResourceKind,
  /// The config of the resource
  pub config_key: uuid::Uuid,
  /// The config of the resource
  pub config: serde_json::Value,
}

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ProxyRule {
  /// Redirect http trafic
  Http(ProxyRuleHttp),
  /// Redirect tcp and udp trafic
  Stream(ProxyRuleStream),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxySslConfig {
  /// Path to the certificate
  pub certificate: String,
  /// Path to the certificate key
  pub certificate_key: String,
  /// Path to the dhparam file
  pub dh_param: Option<String>,
}

/// Defines a proxy rule target
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyTarget {
  /// The cargo key
  pub key: String,
  /// The cargo port
  pub port: u16,
}

/// Proxy rules modes
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ProxyStreamProtocol {
  Tcp,
  Udp,
}

/// Proxy rules modes
#[derive(Debug, Clone)]
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
  /// The target cargo
  pub target: ProxyTarget,
}

/// Defines a proxy rule location
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyHttpLocation {
  /// The path
  pub path: String,
  /// The target cargo
  pub target: ProxyTarget,
}

/// Defines a proxy rule http config
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRuleHttp {
  /// The domain
  pub domain: Option<String>,
  /// Type of private | public | internal | namespace:$namespace_name
  pub r#network: String,
  /// The locations to handle multiple paths
  pub locations: Vec<ProxyHttpLocation>,
  /// The ssl configuration
  pub ssl: Option<ProxySslConfig>,
  /// Path to extra config file to include
  pub includes: Option<Vec<String>>,
}

/// Define proxy rules to apply
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceProxyRule {
  /// Cargo to watch for changes
  pub watch: Vec<String>,
  /// The rule
  #[cfg_attr(feature = "serde", serde(flatten))]
  pub rule: ProxyRule,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceConfig {
  pub key: uuid::Uuid,
  pub resource_key: String,
  pub data: serde_json::Value,
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceQuery {
  pub kind: Option<ResourceKind>,
  pub contains: Option<String>,
}
