mod cargo;
mod namespace;
mod cargo_image;
mod system;
mod setup;

use clap::{Parser, Subcommand};

pub use cargo::*;
pub use namespace::*;
pub use cargo_image::*;
pub use system::*;
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
  Cargo(CargoArgs),
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
