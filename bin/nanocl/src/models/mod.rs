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
mod process;
mod install;
mod uninstall;
mod upgrade;
mod node;
mod context;
mod secret;
mod job;
mod generic;
mod metric;
mod event;

pub use event::*;
pub use generic::*;
pub use process::*;
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
  /// Upgrade components (experimental)
  Upgrade(UpgradeOpts),
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
