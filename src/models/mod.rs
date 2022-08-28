mod utils;
mod cargo;
mod cluster;
mod network;
mod container;
mod namespace;
mod container_image;
mod git_repository;
mod nginx_log;
mod nginx_template;
mod system;
mod node;

use std::io;
use serde::{Serialize, Deserialize};
use clap_complete::{generate, Generator};
use clap::{App, AppSettings, Parser, Subcommand};

pub use cargo::*;
pub use cluster::*;
pub use network::*;
pub use container::*;
pub use namespace::*;
pub use container_image::*;
pub use git_repository::*;
pub use nginx_log::*;
pub use nginx_template::*;
pub use system::*;
pub use node::*;

/// A self-sufficient hybrid-cloud manager
#[derive(Debug, Parser)]
#[clap(
  about,
  version,
  name = "nanocl",
  long_about = "Manage your hybrid cloud with nanocl",
  global_setting = AppSettings::DeriveDisplayOrder,
)]
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
  Docker(DockerOptions),
  Namespace(NamespaceArgs),
  Cluster(ClusterArgs),
  Cargo(CargoArgs),
  Apply(ApplyArgs),
  Revert(RevertArgs),
  GitRepository(GitRepositoryArgs),
  NginxTemplate(NginxTemplateArgs),
  ContainerImage(ContainerImageArgs),
  #[clap(name = "lsc")]
  ListContainer(ListContainerOptions),
  Run(RunArgs),
  Exec(ExecArgs),
  Node(NodeArgs),
  /// Connect to nginx logging
  NginxLog,
  /// Show the Nanocl version information
  Version,
  // TODO shell ompletion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}

/// Alias to self-managed dockerd can be used for debug
#[derive(Debug, Parser)]
pub struct DockerOptions {
  #[clap(multiple = true, raw = true)]
  pub args: Vec<String>,
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

/// TODO for shell completion
pub fn _print_completion<G>(gen: G, app: &mut App)
where
  G: Generator,
{
  generate(gen, app, app.get_name().to_string(), &mut io::stdout());
}
