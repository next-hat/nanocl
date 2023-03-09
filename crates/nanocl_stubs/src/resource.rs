use schemars::JsonSchema;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Resource partial is a payload used to create a new resource
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourcePartial {
  /// The name of the resource
  pub name: String,
  /// The kind of the resource
  pub kind: String,
  /// Version of the config
  pub version: String,
  /// The config of the resource
  pub config: serde_json::Value,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourcePatch {
  /// Version of the config
  pub version: String,
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
  /// The creation date of the resource
  pub created_at: chrono::NaiveDateTime,
  /// The update date of the resource
  pub updated_at: chrono::NaiveDateTime,
  /// Version of the resource
  pub version: String,
  /// The kind of the resource
  pub kind: String,
  /// The config of the resource
  pub config_key: uuid::Uuid,
  /// The config of the resource
  pub config: serde_json::Value,
}

/// Proxy rules modes
#[derive(Debug, Clone, JsonSchema)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ProxyRule {
  /// Redirect http trafic
  Http(ProxyRuleHttp),
  /// Redirect tcp and udp trafic
  Stream(ProxyRuleStream),
}

#[derive(Debug, Clone, JsonSchema)]
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
#[derive(Debug, Clone, JsonSchema)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyTarget {
  /// The cargo key
  pub key: String,
  /// The cargo port
  pub port: u16,
}

/// Proxy rules modes
#[derive(Debug, Clone, JsonSchema)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ProxyStreamProtocol {
  Tcp,
  Udp,
}

/// Proxy rules modes
#[derive(Debug, Clone, JsonSchema)]
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
#[derive(Debug, Clone, JsonSchema)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProxyHttpLocation {
  /// The path
  pub path: String,
  /// The target cargo
  pub target: ProxyTarget,
}

/// Defines a proxy rule http config
#[derive(Debug, Clone, JsonSchema)]
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
#[derive(Debug, Clone, JsonSchema)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceProxyRule {
  /// Cargo to watch for changes
  pub watch: Vec<String>,
  /// The rule
  pub rule: ProxyRule,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceConfig {
  pub key: uuid::Uuid,
  pub version: String,
  pub resource_key: String,
  pub data: serde_json::Value,
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceQuery {
  pub kind: Option<String>,
  pub contains: Option<String>,
}
