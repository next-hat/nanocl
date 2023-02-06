use bollard::exec::CreateExecOptions;
use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::{
  cargo::CargoSummary,
  cargo_config::{
    CargoConfigPatch, ContainerConfig, CargoConfigPartial, ContainerHostConfig,
  },
};

use super::cargo_image::CargoImageOpts;

/// Cargo delete options
#[derive(Debug, Parser)]
pub struct CargoDeleteOpts {
  /// List of cargo names to delete
  pub names: Vec<String>,
}

/// Create cargo options
#[derive(Debug, Clone, Parser)]
pub struct CargoCreateOpts {
  /// Name of the cargo
  pub name: String,
  /// Image of the cargo
  pub image: String,
  /// Network of the cargo this is automatically set to the namespace network
  #[clap(long = "net")]
  pub network: Option<String>,
  /// Volumes of the cargo
  #[clap(short, long = "volume")]
  pub volumes: Option<Vec<String>>,
  /// Environment variables of the cargo
  #[clap(short, long = "env")]
  pub(crate) env: Option<Vec<String>>,
}

impl From<CargoCreateOpts> for CargoConfigPartial {
  fn from(val: CargoCreateOpts) -> Self {
    Self {
      name: val.name,
      container: ContainerConfig {
        image: Some(val.image),
        // network: val.network,
        // volumes: val.volumes,
        env: val.env,
        host_config: Some(ContainerHostConfig {
          network_mode: val.network,
          binds: val.volumes,
          ..Default::default()
        }),
        ..Default::default()
      },
      ..Default::default()
    }
  }
}

/// Start Cargo options
#[derive(Debug, Parser)]
pub struct CargoStartOpts {
  // Name of cargo to start
  pub name: String,
}

/// Stop Cargo options
#[derive(Debug, Parser)]
pub struct CargoStopOpts {
  // Name of cargo to stop
  pub name: String,
}

/// Inspect Cargo options
#[derive(Debug, Parser)]
pub struct CargoInspectOpts {
  /// Name of cargo to inspect
  pub(crate) name: String,
}

/// Patch Cargo options
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

/// Execute a command inside a cargo options
#[derive(Debug, Clone, Parser)]
pub struct CargoExecOpts {
  /// Name of cargo to execute command
  pub name: String,
  /// Command to execute
  pub command: Vec<String>,
}

impl From<CargoExecOpts> for CreateExecOptions<String> {
  fn from(val: CargoExecOpts) -> Self {
    CreateExecOptions {
      cmd: Some(val.command),
      attach_stderr: Some(true),
      attach_stdout: Some(true),
      ..Default::default()
    }
  }
}

#[derive(Debug, Parser)]
pub struct CargoHistoryOpts {
  /// Name of cargo to browse history
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct CargoResetOpts {
  /// Name of cargo to reset
  pub name: String,
  /// Reset to a specific historic
  pub history_id: String,
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
  Image(CargoImageOpts),
  /// Execute a command inside a cargo
  Exec(CargoExecOpts),
  /// List cargo history
  History(CargoHistoryOpts),
  /// Reset cargo to a specific history
  Reset(CargoResetOpts),
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
