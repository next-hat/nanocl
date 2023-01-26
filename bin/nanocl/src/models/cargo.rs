use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocl_stubs::{
  cargo::CargoSummary,
  cargo_config::{CargoConfigPatch, ContainerConfig},
};

use super::cargo_image::CargoImageArgs;

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

/// Cargo stop options
#[derive(Debug, Parser)]
pub struct CargoStopOpts {
  // Name of cargo to stop
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct CargoInspectOpts {
  /// Name of cargo to inspect
  pub(crate) name: String,
}

#[derive(Debug, Clone, Parser)]
pub struct CargoPatchOpts {
  pub(crate) name: String,
  #[clap(short = 'n', long = "name")]
  pub(crate) new_name: Option<String>,
  #[clap(short, long = "image")]
  pub(crate) image: Option<String>,
  #[clap(short, long = "env")]
  pub(crate) env: Option<Vec<String>>,
}

impl From<CargoPatchOpts> for CargoConfigPatch {
  fn from(val: CargoPatchOpts) -> Self {
    CargoConfigPatch {
      name: val.new_name,
      container: Some(ContainerConfig {
        image: val.image,
        env: val.env,
        ..Default::default()
      }),
      ..Default::default()
    }
  }
}

#[derive(Debug, Subcommand)]
#[clap(about, version)]
pub enum CargoCommands {
  /// List existing cargo
  #[clap(alias("ls"))]
  List,
  /// Create a new cargo
  Create(CargoCreateOpts),
  /// Start a cargo by its name
  Start(CargoStartOpts),
  /// Stop a cargo by its name
  Stop(CargoStopOpts),
  /// Remove cargo by its name
  #[clap(alias("rm"))]
  Remove(CargoDeleteOpts),
  /// Inspect a cargo by its name
  Inspect(CargoInspectOpts),
  /// Update a cargo by its name
  Patch(CargoPatchOpts),
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

#[derive(Tabled)]
pub struct CargoRow {
  pub(crate) namespace: String,
  pub(crate) name: String,
  pub(crate) image: String,
  pub(crate) instances: String,
}

impl From<CargoSummary> for CargoRow {
  fn from(cargo: CargoSummary) -> Self {
    Self {
      name: cargo.name,
      namespace: cargo.namespace_name,
      image: cargo.config.container.image.unwrap_or_default(),
      instances: match cargo.config.replication {
        None => format!("{}/{}", cargo.running_instances, 1),
        Some(replication) => format!(
          "{}/{}",
          cargo.running_instances,
          replication.min_replicas.unwrap_or(1)
        ),
      },
    }
  }
}
