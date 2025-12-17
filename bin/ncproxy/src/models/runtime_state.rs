use serde::{Deserialize, Serialize};

/// Persistent state for a single proxy rule backend
/// Used to recreate backends after HAProxy restart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendState {
  /// Backend name (e.g., "bk_deploy-example.global.c-9000")
  pub name: String,
  /// Mode: "http" or "tcp"
  pub mode: String,
  /// Server definitions
  pub servers: Vec<ServerState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerState {
  /// Server name (e.g., "deploy-example.global.c-9000_1")
  pub name: String,
  /// Server address:port
  pub address: String,
  pub port: u16,
  /// Additional options (ssl, alpn, check, etc.)
  pub options: String,
}

/// Persistent state for domain routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainMapEntry {
  /// Domain name (lowercase)
  pub domain: String,
  /// Backend name
  pub backend: String,
}

/// Complete state file for a proxy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRuleState {
  /// Rule name
  pub name: String,
  /// HTTP bind addresses
  pub http_binds: Vec<String>,
  /// HTTPS bind addresses
  pub https_binds: Vec<String>,
  /// All backends for this rule
  pub backends: Vec<BackendState>,
  /// Domain -> backend mappings
  pub domain_mappings: Vec<DomainMapEntry>,
}
