use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use bollard_next::exec::CreateExecOptions;
use nanocld_client::stubs::cargo::CargoSummary;
use nanocld_client::stubs::cargo_config::{
  CargoConfigUpdate, Config as ContainerConfig, CargoConfigPartial, HostConfig,
};

use super::{cargo_image::CargoImageArg, DisplayFormat};

/// ## CargoRemoveOpts
///
/// `nanocl cargo remove` available options
///
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

/// ## CargoCreateOpts
///
/// `nanocl cargo create` available options
///
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

/// Convert CargoCreateOpts to CargoConfigPartial
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

/// ## CargoRunOpts
///
/// `nanocl cargo run` available options
///
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

/// Convert CargoRunOpts to CargoConfigPartial
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

/// ## CargoStartOpts
///
/// `nanocl cargo start` available options
///
#[derive(Debug, Parser)]
pub struct CargoStartOpts {
  // Name of cargo to start
  pub name: String,
}

/// ## CargoStopOpts
///
/// `nanocl cargo stop` available options
///
#[derive(Debug, Parser)]
pub struct CargoStopOpts {
  // List of cargo to stop
  pub names: Vec<String>,
}

/// ## CargoRestartOpts
///
/// `nanocl cargo restart` available options
///
#[derive(Debug, Parser)]
pub struct CargoRestartOpts {
  // List of cargo to stop
  pub names: Vec<String>,
}

/// ## CargoInspectOpts
///
/// `nanocl cargo inspect` available options
///
#[derive(Debug, Parser)]
pub struct CargoInspectOpts {
  /// Display format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
  /// Name of cargo to inspect
  pub(crate) name: String,
}

/// ## CargoPatchOpts
///
/// `nanocl cargo patch` available options
///
#[derive(Debug, Clone, Parser)]
pub struct CargoPatchOpts {
  /// Name of cargo to update
  pub(crate) name: String,
  /// New name of cargo
  #[clap(short = 'n', long = "name")]
  pub(crate) new_name: Option<String>,
  /// New image of cargo
  #[clap(short, long = "image")]
  pub(crate) image: Option<String>,
  /// New environment variables of cargo
  #[clap(short, long = "env")]
  pub(crate) env: Option<Vec<String>>,
  /// New volumes of cargo
  #[clap(short, long = "volume")]
  pub(crate) volumes: Option<Vec<String>>,
}

/// Convert CargoPatchOpts to CargoConfigUpdate
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

/// ## CargoExecOpts
///
/// `nanocl cargo exec` available options
///
#[derive(Debug, Clone, Parser)]
pub struct CargoExecOpts {
  /// Name of cargo to execute command
  pub name: String,
  /// Command to execute
  #[clap(last = true, raw = true)]
  pub command: Vec<String>,
}

/// Convert CargoExecOpts to CreateExecOptions
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

/// ## CargoHistoryOpts
///
/// `nanocl cargo history` available options
///
#[derive(Debug, Parser)]
pub struct CargoHistoryOpts {
  /// Name of cargo to browse history
  pub name: String,
}

/// ## CargoRevertOpts
///
/// `nanocl cargo revert` available options
///
#[derive(Debug, Parser)]
pub struct CargoRevertOpts {
  /// Name of cargo to revert
  pub name: String,
  /// Revert to a specific historic
  pub history_id: String,
}

/// ## CargoLogsOpts
///
/// `nanocl cargo logs` available options
///
#[derive(Debug, Parser)]
pub struct CargoLogsOpts {
  /// Name of cargo to show logs
  pub name: String,
  /// Only include logs since unix timestamp
  #[clap(short = 's')]
  pub since: Option<i64>,
  /// Only include logs until unix timestamp
  #[clap(short = 'u')]
  pub until: Option<i64>,
  /// If integer only return last n logs, if "all" returns all logs
  #[clap(short = 't')]
  pub tail: Option<String>,
  /// Bool, if set include timestamp to ever log line
  #[clap(long = "timestamps")]
  pub timestamps: bool,
  /// Bool, if set open the log as stream
  #[clap(short = 'f')]
  pub follow: bool,
}

/// ## CargoListOpts
///
/// `nanocl cargo list` available options
///
#[derive(Debug, Parser)]
pub struct CargoListOpts {
  /// Only show cargo names
  #[clap(long, short)]
  pub quiet: bool,
}

/// ## CargoCommand
///
/// `nanocl cargo` available commands
///
#[derive(Debug, Subcommand)]
#[clap(about, version)]
pub enum CargoCommand {
  /// List existing cargo
  #[clap(alias("ls"))]
  List(CargoListOpts),
  /// Create a new cargo
  Create(CargoCreateOpts),
  /// Start a cargo by its name
  Start(CargoStartOpts),
  /// Stop a cargo by its name
  Stop(CargoStopOpts),
  /// Restart a cargo by its name
  Restart(CargoRestartOpts),
  /// Remove cargo by its name
  #[clap(alias("rm"))]
  Remove(CargoRemoveOpts),
  /// Inspect a cargo by its name
  Inspect(CargoInspectOpts),
  /// Update a cargo by its name
  Patch(CargoPatchOpts),
  /// Manage cargo image
  Image(CargoImageArg),
  /// Execute a command inside a cargo
  Exec(CargoExecOpts),
  /// List cargo history
  History(CargoHistoryOpts),
  /// Revert cargo to a specific history
  Revert(CargoRevertOpts),
  /// Show logs
  Logs(CargoLogsOpts),
  /// Run a cargo
  Run(CargoRunOpts),
}

/// ## CargoArg
///
/// `nanocl cargo` available arguments
///
#[derive(Debug, Parser)]
#[clap(name = "nanocl cargo")]
pub struct CargoArg {
  /// namespace to target by default global is used
  #[clap(long, short)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub command: CargoCommand,
}

/// ## CargoRow
///
/// A row of the cargo table
///
#[derive(Tabled)]
pub struct CargoRow {
  /// Name of the cargo
  pub(crate) name: String,
  /// Name of the namespace
  pub(crate) namespace: String,
  /// Image of the cargo
  pub(crate) image: String,
  /// Number of running instances
  pub(crate) instances: String,
  /// Config version of the cargo
  pub(crate) config_version: String,
  /// When the cargo was created
  pub(crate) created_at: String,
  /// When the cargo was last updated
  pub(crate) updated_at: String,
}

/// Convert CargoSummary to CargoRow
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
