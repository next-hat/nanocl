mod namespace;
mod cargo;
mod cargo_image;
mod setup;
mod resource;
mod yml;

use clap::{Parser, Subcommand};

pub use yml::*;
pub use cargo::*;
pub use namespace::*;
pub use cargo_image::*;
pub use setup::*;
pub use resource::*;

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
  /// Manage resources
  Resource(ResourceArgs),
  /// Watch daemon events
  Events,
  /// Setup nanocl
  Setup(SetupArgs),
  /// Show nanocl version information
  Version,
  // TODO: shell completion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}
