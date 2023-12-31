use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand, ValueEnum};

mod namespace;
mod cargo;
mod cargo_image;
mod resource;
mod version;
mod state;
mod vm;
mod vm_image;
mod system;
mod install;
mod uninstall;
mod upgrade;
mod node;
mod context;
mod secret;
mod job;
mod generic;
mod metric;

pub use generic::*;
pub use system::*;
pub use metric::*;
pub use secret::*;
pub use context::*;
pub use vm::*;
pub use vm_image::*;
pub use namespace::*;
pub use cargo::*;
pub use cargo_image::*;
pub use resource::*;
pub use state::*;
pub use install::*;
pub use uninstall::*;
pub use upgrade::*;
pub use node::*;
pub use job::*;

/// A self-sufficient hybrid-cloud manager
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

/// `nanocl` available commands
#[derive(Subcommand)]
pub enum Command {
  /// Manage namespaces
  Namespace(NamespaceArg),
  /// Manage jobs
  Job(JobArg),
  /// Manage cargoes
  Cargo(CargoArg),
  /// Manage virtual machines
  Vm(VmArg),
  /// Manage resources
  Resource(ResourceArg),
  /// Manage nodes (experimental)
  Node(NodeArg),
  /// Manage metrics
  Metric(MetricArg),
  /// Watch daemon events
  Events,
  /// Define, Run, or Remove Cargo or Virtual Machines
  State(StateArg),
  /// Manage contexts
  Context(ContextArg),
  /// Show nanocl host information
  Info,
  /// Show nanocl version information
  Version,
  /// Install components
  Install(InstallOpts),
  /// Uninstall components
  Uninstall(UninstallOpts),
  /// Upgrade components
  Upgrade(UpgradeOpts),
  /// Show all processes managed by nanocl
  Ps(ProcessOpts),
  /// Manage secrets
  Secret(SecretArg),
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

/// Convert DisplayFormat to String
impl ToString for DisplayFormat {
  fn to_string(&self) -> String {
    match self {
      Self::Yaml => "yaml",
      Self::Toml => "toml",
      Self::Json => "json",
    }
    .to_owned()
  }
}
