#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Resource kinds
/// It is used to define the kind of a resource
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ResourceKind {
  ProxyRule,
  Unknown,
}

impl std::fmt::Display for ResourceKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ResourceKind::ProxyRule => write!(f, "proxy_rules"),
      ResourceKind::Unknown => write!(f, "unknown"),
    }
  }
}

impl From<String> for ResourceKind {
  fn from(kind: String) -> Self {
    match kind.as_str() {
      "proxy_rules" => ResourceKind::ProxyRule,
      _ => ResourceKind::Unknown,
    }
  }
}

impl From<ResourceKind> for String {
  fn from(kind: ResourceKind) -> Self {
    match kind {
      ResourceKind::ProxyRule => "proxy_rules".into(),
      ResourceKind::Unknown => "unknown".into(),
    }
  }
}

/// Resource partial is a payload used to create a new resource
#[derive(Debug)]
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
/// It is used to define ProxyRules and other resources
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
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProxyRuleMode {
  /// Redirect http trafic
  Http,
  /// Redirect https trafic
  Https,
  /// Redirect tcp trafic
  Tcp,
  /// Redirect udp trafic
  Udp,
}

/// Defines a proxy rule location
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRuleLocation {
  /// The path
  pub path: String,
  /// The target cargo
  pub target_cargo: String,
  /// The target port
  pub target_port: u16,
}

/// Defines a proxy rule http config
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRuleHttpConfig {
  /// The domain
  pub domain: Option<String>,
  /// Ip to listen on
  pub r#type: String,
  /// The locations to handle multiple paths
  pub locations: Vec<ProxyRuleLocation>,
}

/// Defines a proxy rule
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyRule {
  /// The mode of the rule
  pub mode: ProxyRuleMode,
  /// The config depending on the mode
  pub config: serde_json::Value,
}

/// Defines a resource kind `ProxyRule`
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceProxyRule {
  /// Cargo to watch for changes
  pub watch: Vec<String>,
  /// The rules
  pub rules: Vec<ProxyRule>,
}
