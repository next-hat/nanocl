use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

mod backup;
mod cargo;
mod context;
mod event;
mod generic;
mod install;
mod job;
mod metric;
mod namespace;
mod node;
mod process;
mod resource;
mod secret;
mod state;
mod uninstall;
mod version;
mod vm;
mod vm_image;

pub use backup::*;
pub use cargo::*;
pub use context::*;
pub use event::*;
pub use generic::*;
pub use install::*;
pub use job::*;
pub use metric::*;
pub use namespace::*;
pub use node::*;
pub use process::*;
pub use resource::*;
pub use secret::*;
pub use state::*;
pub use uninstall::*;
pub use vm::*;
pub use vm_image::*;

/// Cli available options and commands
#[derive(Parser)]
#[clap(about, version, name = "nanocl")]
pub struct Cli {
  /// Nanocld host default: unix://run/nanocl/nanocl.sock
  #[clap(long, short = 'H')]
  pub host: Option<String>,
  /// Commands
  #[clap(subcommand)]
  pub command: Command,
}

/// Nanocl available commands
#[derive(Subcommand)]
pub enum Command {
  /// Manage namespaces
  Namespace(NamespaceArg),
  /// Manage secrets
  Secret(SecretArg),
  /// Manage jobs
  Job(JobArg),
  /// Manage cargoes
  Cargo(CargoArg),
  /// Manage virtual machines
  Vm(VmArg),
  /// Manage resources
  Resource(ResourceArg),
  /// Manage metrics
  Metric(MetricArg),
  /// Manage contexts
  Context(ContextArg),
  /// Manage nodes (experimental)
  Node(NodeArg),
  /// Apply or Remove a Statefile
  State(StateArg),
  /// Show or watch events
  Event(EventArg),
  /// Show processes
  Ps(GenericListOpts<ProcessFilter>),
  /// Show nanocl host information
  Info,
  /// Show nanocl version information
  Version,
  /// Install components
  Install(InstallOpts),
  /// Uninstall components
  Uninstall(UninstallOpts),
  /// Backup the current state
  Backup(BackupOpts),
  // TODO: shell completion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}

/// `nanocl` available display formats `yaml` by default
#[derive(Default, Clone, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "PascalCase")]
pub enum DisplayFormat {
  #[default]
  Yaml,
  Toml,
  Json,
}

impl std::fmt::Display for DisplayFormat {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let data = match self {
      Self::Yaml => "yaml",
      Self::Toml => "toml",
      Self::Json => "json",
    };
    write!(f, "{data}")
  }
}
