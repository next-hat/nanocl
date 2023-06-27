use std::collections::HashMap;

use tabled::Tabled;
use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

/// Manage contexts
#[derive(Debug, Parser)]
pub struct ContextArgs {
  #[clap(subcommand)]
  pub commands: ContextCommands,
}

#[derive(Debug, Subcommand)]
pub enum ContextCommands {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContextEndpoint {
  pub host: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContextMetaData {
  pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Context {
  pub name: String,
  pub meta_data: ContextMetaData,
  pub endpoints: HashMap<String, ContextEndpoint>,
}

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
              .unwrap_or("unix://run/nanocl/nanocl.sock".into()),
          },
        );
        map
      },
    }
  }
}

#[derive(Clone, Debug, Tabled)]
pub struct ContextRow {
  pub name: String,
  pub description: String,
  pub endpoint: String,
  pub current: String,
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DockerContextMetaEndpoint {
  pub host: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DockerContextMeta {
  pub endpoints: HashMap<String, DockerContextMetaEndpoint>,
}
