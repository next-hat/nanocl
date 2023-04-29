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
// mod upgrade;

pub use system::*;
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
// pub use upgrade::*;

use clap::{Parser, Subcommand};

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
  /// Manage namespaces
  Namespace(NamespaceArgs),
  /// Manage cargoes
  Cargo(CargoArgs),
  /// Manage virtual machines
  Vm(VmArgs),
  /// Manage resources
  Resource(ResourceArgs),
  /// Watch daemon events
  Events,
  /// Apply or Reverse a state from a configuration file
  State(StateArgs),
  /// Show nanocl host information
  Info,
  /// Show nanocl version information
  Version,
  /// Install nanocl components
  Install(InstallOpts),
  /// Uninstall nanocl components
  Uninstall(UninstallOpts),
  /// Upgrade nanocl components
  Upgrade,
  /// Show all processes managed by nanocl
  Ps(ProcessOpts),
  // TODO: shell completion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}
