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

pub use system::*;
pub use context::*;
pub use vm::*;
pub use vm_image::*;
pub use namespace::*;
pub use cargo::*;
pub use cargo_image::*;
pub use resource::*;
pub use version::*;
pub use state::*;
pub use install::*;
pub use uninstall::*;
pub use upgrade::*;
pub use node::*;

/// A self-sufficient hybrid-cloud manager
#[derive(Debug, Parser)]
#[clap(about, version, name = "nanocl")]
pub struct Cli {
  /// Nanocld host default: unix://run/nanocl/nanocl.sock
  #[clap(long, short = 'H')]
  pub host: Option<String>,
  /// Commands
  #[clap(subcommand)]
  pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
  /// Manage namespaces
  Namespace(NamespaceArgs),
  /// Manage cargoes
  Cargo(CargoArgs),
  /// Manage virtual machines
  Vm(VmArgs),
  /// Manage resources
  Resource(ResourceArgs),
  /// Manage nodes (experimental)
  Node(NodeArgs),
  /// Watch daemon events
  Events,
  /// Define, Run, or Remove Cargo or Virtual Machines
  State(StateArgs),
  /// Manage contexts
  Context(ContextArgs),
  /// Show nanocl host information
  Info,
  /// Show nanocl version information
  Version,
  /// Install nanocl components
  Install(InstallOpts),
  /// Uninstall nanocl components
  Uninstall(UninstallOpts),
  /// Upgrade nanocl components
  Upgrade(UpgradeOpts),
  /// Show all processes managed by nanocl
  Ps(ProcessOpts),
  /// Manage system
  System(SystemOpts),
  // TODO: shell completion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}

#[derive(Clone, Debug, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "PascalCase")]
pub enum DisplayFormat {
  Yaml,
  Toml,
  Json,
}

impl ToString for DisplayFormat {
  fn to_string(&self) -> String {
    match self {
      Self::Yaml => "yaml",
      Self::Toml => "toml",
      Self::Json => "json",
    }
    .to_string()
  }
}

impl Default for DisplayFormat {
  fn default() -> Self {
    Self::Yaml
  }
}
