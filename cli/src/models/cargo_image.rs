use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct CargoImageRemoveOpts {
  /// id or name of image to delete
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct CargoImageCreateOpts {
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct CargoImageInspectOpts {
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum CargoImageCommands {
  /// List cargo images
  #[clap(alias("ls"))]
  List,
  /// Create a new cargo image
  Create(CargoImageCreateOpts),
  /// Remove an existing cargo image
  #[clap(alias("rm"))]
  Remove(CargoImageRemoveOpts),
  /// Inspect a cargo image
  Inspect(CargoImageInspectOpts),
}

/// Manage container images
#[derive(Debug, Parser)]
pub struct CargoImageArgs {
  #[clap(subcommand)]
  pub(crate) commands: CargoImageCommands,
}
