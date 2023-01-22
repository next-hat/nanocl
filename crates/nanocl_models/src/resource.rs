#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[cfg(feature = "diesel")]
use diesel_derive_enum::DbEnum;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "diesel", derive(DbEnum))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "diesel", DbValueStyle = "snake_case")]
pub enum ResourceKind {
  ProxyRule,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourcePartial {
  pub name: String,
  pub kind: ResourceKind,
  pub config: serde_json::Value,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Resource {
  pub name: String,
  pub kind: ResourceKind,
  pub config_key: uuid::Uuid,
  pub config: serde_json::Value,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProxyRuleMode {
  Http,
  Https,
  Tcp,
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
  pub listen_on: String,
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
  pub name: String,
  pub watch: Vec<String>,
  pub rules: Vec<ProxyRule>,
}
