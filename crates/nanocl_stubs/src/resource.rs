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
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ProxyRule {
  /// Redirect http trafic
  Http(ProxyRuleHttp),
  /// Redirect https trafic
  Https(ProxyRuleHttp),
  /// Redirect tcp trafic
  Tcp,
  /// Redirect udp trafic
  Udp,
}

/// Defines a proxy rule target
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyTarget {
  /// The cargo namespace
  pub namespace: Option<String>,
  /// The cargo name
  pub name: String,
  /// The cargo port
  pub port: u16,
}

/// Defines a proxy rule location
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRuleLocation {
  /// The path
  pub path: String,
  /// The target cargo
  pub target: ProxyTarget,
}

/// Defines a proxy rule http config
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRuleHttp {
  /// The domain
  pub domain: Option<String>,
  /// Type of private | public | internal
  pub r#type: String,
  /// The locations to handle multiple paths
  pub locations: Vec<ProxyRuleLocation>,
}

/// Define cargo to watch for changes
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyWatch {
  /// The cargo namespace
  pub namespace: Option<String>,
  /// The cargo name
  pub name: String,
}

/// Define proxy rules to apply
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceProxyRule {
  /// Cargo to watch for changes
  pub watch: Vec<ProxyWatch>,
  /// The rule
  #[serde(flatten)]
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
