use std::collections::HashMap;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tabled::Tabled;

use nanocld_client::stubs::system::SslConfig;

/// `nanocl context` available arguments
#[derive(Parser)]
pub struct ContextArg {
  #[clap(subcommand)]
  pub command: ContextCommand,
}

/// `nanocl context` available commands
#[derive(Subcommand)]
pub enum ContextCommand {
  /// List contexts
  #[clap(alias = "ls")]
  List,
  /// Set current context
  Use {
    /// Context name
    name: String,
  },
  /// Create a new context from a file
  From {
    /// Path to context file
    path: String,
  },
}

/// A context endpoint definition
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContextEndpoint {
  pub host: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ssl: Option<SslConfig>,
}

/// A context metadata definition
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContextMetaData {
  pub description: String,
}

/// A context definition is a user defined set of endpoints to manage remote nanocl clusters
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Context {
  pub name: String,
  pub meta_data: ContextMetaData,
  pub endpoints: HashMap<String, ContextEndpoint>,
}

/// Default value for a Context
impl std::default::Default for Context {
  fn default() -> Self {
    Self {
      name: "default".into(),
      meta_data: ContextMetaData {
        description: "Default context based on NANOCL_HOST env".into(),
      },
      endpoints: {
        let mut map = HashMap::new();
        map.insert(
          "Nanocl".into(),
          ContextEndpoint {
            host: std::env::var("NANOCL_HOST")
              .unwrap_or("unix:///run/nanocl/nanocl.sock".into()),
            ssl: None,
          },
        );
        map
      },
    }
  }
}

/// A row of the context table
#[derive(Clone, Tabled)]
pub struct ContextRow {
  /// Name of the context
  pub name: String,
  /// Description of the context
  pub description: String,
  /// Endpoint of the context
  pub endpoint: String,
  /// Current context indicator
  pub current: String,
}

/// Convert Context to ContextRow
impl From<Context> for ContextRow {
  fn from(context: Context) -> Self {
    let endpoint = context
      .endpoints
      .get("Nanocl")
      .expect("Expect context to have a Nanocl endpoint");
    Self {
      name: context.name,
      description: context.meta_data.description,
      endpoint: endpoint.host.clone(),
      current: "⨯".into(),
    }
  }
}

/// A docker context endpoint definition used to parse the docker context metadata endpoint
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DockerContextMetaEndpoint {
  pub host: String,
}

/// A docker context metadata definition used to parse the docker context metadata
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DockerContextMeta {
  pub endpoints: HashMap<String, DockerContextMetaEndpoint>,
}
