use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

use super::cargo_image::CargoImageArgs;
use super::cargo_config::{CargoConfig, CargoConfigPartial};

/// Cargo delete options
#[derive(Debug, Parser)]
pub struct CargoDeleteOpts {
  /// List of cargo names to delete
  pub names: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct CargoCreateOpts {
  /// Name of the cargo
  pub name: String,
  /// Image of the cargo
  pub image: String,
  /// Network of the cargo
  #[clap(long = "net")]
  pub network: Option<String>,
  /// Volumes of the cargo
  #[clap(short)]
  pub volumes: Option<Vec<String>>,
}

/// Cargo start options
#[derive(Debug, Parser)]
pub struct CargoStartOpts {
  // Name of cargo to start
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct CargoInspectOpts {
  /// Name of cargo to inspect
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum CargoPatchCommands {}

#[derive(Debug, Parser)]
pub struct CargoPatchArgs {
  pub(crate) name: String,
  #[clap(subcommand)]
  pub(crate) commands: CargoPatchCommands,
}

#[derive(Debug, Subcommand)]
#[clap(about, version)]
pub enum CargoCommands {
  /// List existing cargo
  #[clap(alias("ls"))]
  List,
  /// Create a new cargo
  Create(CargoCreateOpts),
  /// Remove cargo by it's name
  #[clap(alias("rm"))]
  Remove(CargoDeleteOpts),
  /// Inspect a cargo by it's name
  Inspect(CargoInspectOpts),
  /// Update a cargo by it's name
  Patch(CargoPatchArgs),
  /// Manage cargo image
  Image(CargoImageArgs),
}

/// Manage cargoes
#[derive(Debug, Parser)]
#[clap(name = "nanocl-cargo")]
pub struct CargoArgs {
  /// namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: CargoCommands,
}

/// Structure to create a cargo used as body
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
#[cfg_attr(feature = "dev", derive(ToSchema))]
pub struct CargoPartial {
  pub(crate) name: String,
  pub(crate) config: CargoConfigPartial,
}

/// Cargo with his current config
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[cfg_attr(feature = "dev", derive(ToSchema))]
pub struct Cargo {
  pub(crate) key: String,
  pub(crate) namespace_name: String,
  pub(crate) name: String,
  pub(crate) config_key: String,
  pub(crate) config: CargoConfig,
}
