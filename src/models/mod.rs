mod utils;
mod cargo;
mod cluster;
mod network;
mod cargo_instance;
mod namespace;
mod cargo_image;
mod git_repository;
mod proxy;
mod system;
mod yml;
mod controller;
mod setup;

use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand};

pub use cargo::*;
pub use cluster::*;
pub use network::*;
pub use cargo_instance::*;
pub use namespace::*;
pub use cargo_image::*;
pub use git_repository::*;
pub use controller::*;
pub use proxy::*;
pub use system::*;
pub use yml::*;
pub use setup::*;

/// A self-sufficient hybrid-cloud manager
#[derive(Debug, Parser)]
#[clap(about, version, name = "nanocl")]
pub struct Cli {
  /// Nanocld host
  #[clap(long, short = 'H', default_value = "unix://run/nanocl/nanocl.sock")]
  pub host: String,
  /// Commands
  #[clap(subcommand)]
  pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
  Namespace(NamespaceArgs),
  Cluster(ClusterArgs),
  Cargo(CargoArgs),
  Apply(ApplyArgs),
  Revert(RevertArgs),
  Run(RunArgs),
  Exec(ExecArgs),
  Proxy(ProxyArgs),
  Controller(ControllerArgs),
  Setup(SetupArgs),
  /// Show nanocl version information
  Version,
  // TODO shell completion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}

/// Apply a configuration file
#[derive(Debug, Parser)]
#[clap(name = "nanocl-apply")]
pub struct ApplyArgs {
  #[clap(short)]
  /// .yml conf file to apply
  pub(crate) file_path: String,
}

/// Revert a configuration file
#[derive(Debug, Parser)]
#[clap(name = "nanocl-revert")]
pub struct RevertArgs {
  #[clap(short)]
  /// .yml conf file to revert
  pub(crate) file_path: String,
}

/// Run a cargo in given environement
#[derive(Debug, Parser)]
pub struct RunArgs {
  #[clap(long)]
  pub(crate) namespace: Option<String>,
  #[clap(long)]
  pub(crate) cluster: String,
  #[clap(long)]
  pub(crate) network: String,
  #[clap(long)]
  pub(crate) image: String,
  pub(crate) name: String,
}

/// Generic database delete response
#[derive(Debug, Serialize, Deserialize)]
pub struct PgGenericDelete {
  pub(crate) count: usize,
}

/// Generic database count response
#[derive(Debug, Serialize, Deserialize)]
pub struct PgGenericCount {
  pub(crate) count: usize,
}

/// Generic url query with optional namespace
#[derive(Debug, Serialize, Deserialize)]
pub struct GenericNamespaceQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) namespace: Option<String>,
}
