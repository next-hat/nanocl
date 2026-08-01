use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use bollard_next::exec::CreateExecOptions;

use nanocld_client::stubs::{
  cargo::{CargoKillOptions, CargoSummary},
  cargo_spec::{CargoSpecPartial, CargoSpecUpdate, Config, HostConfig},
};

use super::{
  GenericInspectOpts, GenericRemoveForceOpts, GenericRemoveOpts,
  GenericStartOpts, GenericStopOpts, NamespacedListOpts,
};

/// `nanocl cargo create` available options
#[derive(Clone, Parser)]
pub struct CargoCreateOpts {
  /// Namespace for the new cargo; defaults to global
  #[clap(short, long)]
  pub namespace: Option<String>,
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

/// Convert CargoCreateOpts to CargoSpecPartial
impl From<CargoCreateOpts> for CargoSpecPartial {
  fn from(val: CargoCreateOpts) -> Self {
    Self {
      name: val.name,
      container: Config {
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

/// `nanocl cargo run` available options
#[derive(Clone, Parser)]
pub struct CargoRunOpts {
  /// Namespace for the new cargo; defaults to global
  #[clap(short, long)]
  pub namespace: Option<String>,
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

/// Convert CargoRunOpts to CargoSpecPartial
impl From<CargoRunOpts> for CargoSpecPartial {
  fn from(val: CargoRunOpts) -> Self {
    Self {
      name: val.name,
      container: Config {
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

/// `nanocl cargo restart` available options
#[derive(Clone, Parser)]
pub struct CargoRestartOpts {
  // Canonical keys of cargoes to restart
  pub keys: Vec<String>,
}

/// `nanocl cargo patch` available options
#[derive(Clone, Parser)]
pub struct CargoPatchOpts {
  /// Canonical key of the cargo to update
  pub(crate) key: String,
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

/// Convert CargoPatchOpts to CargoSpecUpdate
impl From<CargoPatchOpts> for CargoSpecUpdate {
  fn from(val: CargoPatchOpts) -> Self {
    CargoSpecUpdate {
      name: None,
      container: Some(Config {
        image: val.image,
        env: val.env,
        ..Default::default()
      }),
      ..Default::default()
    }
  }
}

/// `nanocl cargo exec` available options
#[derive(Clone, Parser)]
pub struct CargoExecOpts {
  /// Allocate a pseudo-TTY.
  #[clap(short = 't', long = "tty")]
  pub tty: bool,
  /// Canonical key of the cargo in which to execute the command
  pub key: String,
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

/// `nanocl cargo history` available options
#[derive(Clone, Parser)]
pub struct CargoHistoryOpts {
  /// Canonical key of the cargo whose history to browse
  pub key: String,
}

/// `nanocl cargo revert` available options
#[derive(Clone, Parser)]
pub struct CargoRevertOpts {
  /// Canonical key of the cargo to revert
  pub key: String,
  /// Revert to a specific historic
  pub history_id: String,
}

/// `nanocl cargo logs` available options
#[derive(Clone, Parser)]
pub struct CargoLogsOpts {
  /// Canonical key of the cargo whose logs to show
  pub key: String,
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

/// `nanocl cargo stats` available options
#[derive(Clone, Parser)]
pub struct CargoStatsOpts {
  /// Canonical keys of cargoes whose stats to show
  pub keys: Vec<String>,
  /// Disable streaming stats and only pull the first result
  #[clap(long)]
  pub no_stream: bool,
  // TODO: Show all containers (default shows just running)
  // pub all: bool,
}

/// `nanocl cargo kill` available options
#[derive(Clone, Parser)]
pub struct CargoKillOpts {
  /// Canonical key of the cargo to signal
  pub key: String,
  /// Signal to send
  #[clap(short, long, default_value = "SIGKILL")]
  pub signal: String,
}

impl From<CargoKillOpts> for CargoKillOptions {
  fn from(value: CargoKillOpts) -> Self {
    Self {
      signal: value.signal,
    }
  }
}

/// `nanocl cargo` available commands
#[derive(Clone, Subcommand)]
pub enum CargoCommand {
  /// List existing cargo
  #[clap(alias("ls"))]
  List(NamespacedListOpts),
  /// Create a new cargo
  Create(CargoCreateOpts),
  /// Start cargoes by canonical keys
  Start(GenericStartOpts),
  /// Stop cargoes by canonical keys
  Stop(GenericStopOpts),
  /// Restart cargoes by canonical keys
  Restart(CargoRestartOpts),
  /// Remove cargoes by canonical keys
  #[clap(alias("rm"))]
  Remove(GenericRemoveOpts<GenericRemoveForceOpts>),
  /// Inspect a cargo by its canonical key
  Inspect(GenericInspectOpts),
  /// Update a cargo by its canonical key
  Patch(CargoPatchOpts),
  /// Execute a command inside a cargo
  Exec(CargoExecOpts),
  /// List cargo history
  History(CargoHistoryOpts),
  /// Revert cargo to a specific history
  Revert(CargoRevertOpts),
  /// Show logs
  Logs(CargoLogsOpts),
  /// Send a signal to a cargo by its canonical key
  Kill(CargoKillOpts),
  /// Run a cargo
  Run(CargoRunOpts),
  /// Show stats of cargo
  Stats(CargoStatsOpts),
}

/// `nanocl cargo` available arguments
#[derive(Clone, Parser)]
pub struct CargoArg {
  #[clap(subcommand)]
  pub command: CargoCommand,
}

/// A row of the cargo table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct CargoRow {
  /// Canonical key of the cargo
  pub(crate) key: String,
  /// Image of the cargo
  pub(crate) image: String,
  /// Status of the cargo
  pub(crate) status: String,
  /// Number of running instances
  pub(crate) instances: String,
  /// Spec version of the cargo
  pub(crate) version: String,
  /// When the cargo was created
  #[tabled(rename = "CREATED AT")]
  pub(crate) created_at: String,
  /// When the cargo was last updated
  #[tabled(rename = "UPDATED AT")]
  pub(crate) updated_at: String,
}

/// Convert CargoSummary to CargoRow
impl From<CargoSummary> for CargoRow {
  fn from(cargo: CargoSummary) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(cargo.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(cargo.spec.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: cargo.spec.cargo_key,
      image: cargo.spec.container.image.unwrap_or_default(),
      version: cargo.spec.version,
      status: format!(
        "{}/{} ({})",
        cargo.status.actual, cargo.status.wanted, cargo.status.health
      ),
      instances: format!("{}/{}", cargo.instance_running, cargo.instance_total),
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  #[test]
  fn namespace_is_only_available_for_collection_and_creation_commands() {
    let inspect =
      CargoArg::try_parse_from(["cargo", "inspect", "system.same"]).unwrap();
    assert!(matches!(inspect.command, CargoCommand::Inspect(_)));
    assert!(
      CargoArg::try_parse_from([
        "cargo",
        "--namespace",
        "system",
        "inspect",
        "system.same"
      ])
      .is_err()
    );

    let list =
      CargoArg::try_parse_from(["cargo", "list", "--namespace", "system"])
        .unwrap();
    let CargoCommand::List(list) = list.command else {
      panic!("expected list command");
    };
    assert_eq!(list.namespace.as_deref(), Some("system"));

    let create =
      CargoArg::try_parse_from(["cargo", "create", "new", "alpine"]).unwrap();
    let CargoCommand::Create(create) = create.command else {
      panic!("expected create command");
    };
    assert_eq!(create.namespace, None);
  }

  #[test]
  fn table_rows_distinguish_duplicate_local_names() {
    let table = tabled::Table::new([
      CargoRow {
        key: "global.same".to_owned(),
        image: "alpine".to_owned(),
        status: "running".to_owned(),
        instances: "1/1".to_owned(),
        version: "v1".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
      },
      CargoRow {
        key: "system.same".to_owned(),
        image: "alpine".to_owned(),
        status: "running".to_owned(),
        instances: "1/1".to_owned(),
        version: "v1".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
      },
    ])
    .to_string();
    assert!(table.contains("KEY"));
    assert!(table.contains("global.same"));
    assert!(table.contains("system.same"));
  }
}
