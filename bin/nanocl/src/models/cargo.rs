use bollard_next::exec::CreateExecOptions;
use chrono::TimeZone;
use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::{
  cargo::CargoSummary,
  cargo_config::{
    CargoConfigUpdate, Config as ContainerConfig, CargoConfigPartial,
    HostConfig,
  },
};

use super::cargo_image::CargoImageOpts;

/// Cargo delete options
#[derive(Debug, Parser)]
pub struct CargoRemoveOpts {
  /// Skip confirmation
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// Force delete
  #[clap(short = 'f')]
  pub force: bool,
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
        host_config: Some(HostConfig {
          binds: val.volumes,
          ..Default::default()
        }),
        ..Default::default()
      },
      ..Default::default()
    }
  }
}

#[derive(Debug, Clone, Parser)]
pub struct CargoRunOpts {
  /// Name of the cargo
  pub name: String,
  /// Image of the cargo
  pub image: String,
  /// Volumes of the cargo
  #[clap(short, long = "volume")]
  pub volumes: Option<Vec<String>>,
  /// Environment variables of the cargo
  #[clap(short, long = "env")]
  pub env: Option<Vec<String>>,
  #[clap(long = "rm", default_value = "false")]
  pub auto_remove: bool,
  /// Command to execute
  pub command: Vec<String>,
}

impl From<CargoRunOpts> for CargoConfigPartial {
  fn from(val: CargoRunOpts) -> Self {
    Self {
      name: val.name,
      container: ContainerConfig {
        image: Some(val.image),
        // network: val.network,
        // volumes: val.volumes,
        env: val.env,
        cmd: Some(val.command),
        host_config: Some(HostConfig {
          binds: val.volumes,
          auto_remove: Some(val.auto_remove),
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
  // List of cargo to stop
  pub names: Vec<String>,
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
  #[clap(short, long = "volume")]
  pub(crate) volumes: Option<Vec<String>>,
}

impl From<CargoPatchOpts> for CargoConfigUpdate {
  fn from(val: CargoPatchOpts) -> Self {
    CargoConfigUpdate {
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
  #[clap(last = true, raw = true)]
  pub command: Vec<String>,
}

impl From<CargoExecOpts> for CreateExecOptions {
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

#[derive(Debug, Parser)]
pub struct CargoLogsOpts {
  /// Name of cargo to show logs
  pub name: String,
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
  Remove(CargoRemoveOpts),
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
  /// Show logs
  Logs(CargoLogsOpts),
  /// Run a cargo
  Run(CargoRunOpts),
}

/// Manage cargoes
#[derive(Debug, Parser)]
#[clap(name = "nanocl cargo")]
pub struct CargoArgs {
  /// namespace to target by default global is used
  #[clap(long, short)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: CargoCommands,
}

#[derive(Tabled)]
pub struct CargoRow {
  pub(crate) name: String,
  pub(crate) namespace: String,
  pub(crate) image: String,
  pub(crate) instances: String,
  pub(crate) config_version: String,
  pub(crate) created_at: String,
  pub(crate) updated_at: String,
}

impl From<CargoSummary> for CargoRow {
  fn from(cargo: CargoSummary) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(cargo.created_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(cargo.updated_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      name: cargo.name,
      namespace: cargo.namespace_name,
      image: cargo.config.container.image.unwrap_or_default(),
      config_version: cargo.config.version,
      instances: format!("{}/{}", cargo.instance_running, cargo.instance_total),
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}
