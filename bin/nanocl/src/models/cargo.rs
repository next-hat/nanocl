use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use bollard_next::exec::CreateExecOptions;
use bollard_next::container::MemoryStatsStats;
use nanocld_client::stubs::cargo::{CargoStats, CargoSummary};
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
  /// Allocate a pseudo-TTY.
  #[clap(short = 't', long = "tty")]
  pub tty: bool,
  /// Name of cargo to execute command
  pub name: String,
  /// Command to execute
  #[clap(last = true, raw = true)]
  pub command: Vec<String>,
  /// Override the key sequence for detaching a container.
  #[clap(long)]
  pub detach_keys: Option<String>,
  /// Set environment variables
  #[clap(short)]
  pub env: Option<Vec<String>>,
  /// Give extended privileges to the command
  #[clap(long)]
  pub privileged: bool,
  /// Username or UID (format: "<name|uid>[:<group|gid>]")
  #[clap(short)]
  pub user: Option<String>,
  /// Working directory inside the container
  #[clap(short, long = "workdir")]
  pub working_dir: Option<String>,
}

/// Convert CargoExecOpts to CreateExecOptions
impl From<CargoExecOpts> for CreateExecOptions {
  fn from(val: CargoExecOpts) -> Self {
    CreateExecOptions {
      cmd: Some(val.command),
      tty: Some(val.tty),
      detach_keys: val.detach_keys,
      env: val.env,
      privileged: Some(val.privileged),
      user: val.user,
      working_dir: val.working_dir,
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

/// ## CargoStatsOpts
///
/// `nanocl cargo stats` available options
///
#[derive(Debug, Parser)]
pub struct CargoStatsOpts {
  /// Names of cargo to show stats
  pub names: Vec<String>,
  /// Disable streaming stats and only pull the first result
  #[clap(long)]
  pub no_stream: bool,
  // TODO: Show all containers (default shows just running)
  // pub all: bool,
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
  /// Show stats of cargo
  Stats(CargoStatsOpts),
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

#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct CargoStatsRow {
  key: String,
  #[tabled(rename = "CPU %")]
  cpu_usage: String,
  #[tabled(rename = "MEM USAGE / LIMIT")]
  mem_usage_limit: String,
  #[tabled(rename = "MEM %")]
  mem: String,
  #[tabled(rename = "NET I/O")]
  net_io: String,
  #[tabled(rename = "BLOCK I/O")]
  block_io: String,
  pids: String,
}

impl From<CargoStats> for CargoStatsRow {
  fn from(stats: CargoStats) -> Self {
    let key = stats.name.replace('/', "");
    let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
      - stats.precpu_stats.cpu_usage.total_usage as f64;
    let system_cpu_delta = stats.cpu_stats.system_cpu_usage.unwrap_or_default()
      as f64
      - stats.precpu_stats.system_cpu_usage.unwrap_or_default() as f64;
    let number_cpus = stats.cpu_stats.online_cpus.unwrap_or_default() as f64;
    let cpu_usage = format!(
      "{:.2}%",
      ((cpu_delta / system_cpu_delta) * number_cpus) * 100.0
    );
    let available_memory = stats.memory_stats.limit.unwrap_or_default() as f64;
    let used_memory = stats.memory_stats.usage.unwrap_or_default() as f64;
    let memory_usage = if let Some(memory_stats) = stats.memory_stats.stats {
      match memory_stats {
        MemoryStatsStats::V1(mem_stat) => used_memory - mem_stat.cache as f64,
        MemoryStatsStats::V2(mem_stat) => {
          used_memory - mem_stat.inactive_file as f64
        }
      }
    } else {
      0.00
    };
    let net_io = if let Some(networks) = stats.networks {
      // calculate total network io
      let mut total_rx = 0;
      let mut total_tx = 0;
      for (_, network) in networks {
        total_rx += network.rx_bytes;
        total_tx += network.tx_bytes;
      }
      format!(
        "{:.1}MB / {:.1}MB",
        // convert to MB
        total_rx as f64 / 1000.00 / 1000.00,
        // convert to MB
        total_tx as f64 / 1000.00 / 1000.00
      )
    } else {
      String::default()
    };
    let (total_read, total_write) = if let Some(io_service_bytes_recursive) =
      stats.blkio_stats.io_service_bytes_recursive
    {
      (
        io_service_bytes_recursive[0].value as f64,
        io_service_bytes_recursive[1].value as f64,
      )
    } else {
      (0.00, 0.00)
    };
    let block_io = format!(
      "{:.1}MB / {:.1}GB",
      total_read / 1000.00 / 1000.00,
      total_write / 1000.00 / 1000.00 / 1000.00
    );
    let pids = format!("{}", stats.pids_stats.current.unwrap_or_default());
    Self {
      key,
      cpu_usage,
      mem_usage_limit: format!(
        "{:.1}MiB / {:.2}GiB",
        // convert to MiB
        memory_usage / 1024.00 / 1024.00,
        // convert to GiB
        available_memory / 1024.00 / 1024.00 / 1024.00
      ),
      mem: format!("{:.2}%", (memory_usage / available_memory) * 100.0),
      net_io,
      block_io,
      pids,
    }
  }
}
