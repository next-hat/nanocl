use clap::{Parser, Subcommand, ValueEnum};
use nanocld_client::stubs::process::ProcessLogQuery;
use serde::{Deserialize, Serialize};

mod backup;
mod cargo;
mod context;
mod event;
mod generic;
mod install;
mod job;
mod metric;
mod namespace;
mod node;
mod process;
mod resource;
mod secret;
mod state;
mod stats;
mod uninstall;
mod version;
mod vm;
mod vm_image;

pub use backup::*;
pub use cargo::*;
pub use context::*;
pub use event::*;
pub use generic::*;
pub use install::*;
pub use job::*;
pub use metric::*;
pub use namespace::*;
pub use node::*;
pub use process::*;
pub use resource::*;
pub use secret::*;
pub use state::*;
pub use stats::*;
pub use uninstall::*;
pub use vm::*;
pub use vm_image::*;

const LONG_ABOUT: &str = r#"Nanocl is a modern, self-sufficient orchestrator for containers and virtual machines.
It delivers a clean dev→prod workflow with declarative Statefiles (YAML/TOML/JSON) and opinionated, predictable defaults.
Built in Rust for performance and safety, it keeps operational overhead low while remaining powerful and extensible.
Manage cargoes, resources, jobs, and VMs with dynamic routing, DNS, and end-to-end TLS.
Start local on a single node and scale out when ready — simple to learn, production-grade by design."#;

/// Cli available options and commands
#[derive(Parser)]
#[clap(name = "nanocl", version, about, long_about = LONG_ABOUT)]
pub struct Cli {
  /// Nanocld host default: unix://run/nanocl/nanocl.sock
  #[clap(long, short = 'H')]
  pub host: Option<String>,
  /// Commands
  #[clap(subcommand)]
  pub command: Command,
}

/// `nanocl logs` available options
#[derive(Clone, Parser)]
pub struct LogsOpts {
  /// Name(s) of processes to show logs
  pub names: Vec<String>,
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

impl From<LogsOpts> for ProcessLogQuery {
  fn from(opts: LogsOpts) -> Self {
    Self {
      namespace: None,
      since: opts.since,
      until: opts.until,
      tail: opts.tail,
      timestamps: Some(opts.timestamps),
      follow: Some(opts.follow),
      stderr: Some(true),
      stdout: Some(true),
    }
  }
}

/// Nanocl available commands
#[derive(Subcommand)]
pub enum Command {
  /// Manage namespaces
  Namespace(NamespaceArg),
  /// Manage secrets
  Secret(SecretArg),
  /// Manage jobs
  Job(JobArg),
  /// Manage cargoes
  Cargo(CargoArg),
  /// Manage virtual machines
  Vm(VmArg),
  /// Manage resources
  Resource(ResourceArg),
  /// Manage metrics
  Metric(MetricArg),
  /// Manage contexts
  Context(ContextArg),
  /// Manage nodes (experimental)
  Node(NodeArg),
  /// Apply or Remove a Statefile
  State(StateArg),
  /// Show or watch events
  Event(EventArg),
  /// Show processes
  Ps(GenericListOpts<ProcessFilter>),
  /// Get logs of a process
  Logs(LogsOpts),
  /// Inspect a process
  Inspect(GenericInspectOpts),
  /// Show nanocl host information
  Info,
  /// Show nanocl version information
  Version,
  /// Install components
  Install(InstallOpts),
  /// Uninstall components
  Uninstall(UninstallOpts),
  /// Backup the current state
  Backup(BackupOpts),
  /// Stats of the process
  Stats(StatsOpts),
  // TODO: shell completion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}

/// `nanocl` available display formats `yaml` by default
#[derive(Default, Clone, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "PascalCase")]
pub enum DisplayFormat {
  #[default]
  Yaml,
  Toml,
  Json,
}

impl std::fmt::Display for DisplayFormat {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let data = match self {
      Self::Yaml => "yaml",
      Self::Toml => "toml",
      Self::Json => "json",
    };
    write!(f, "{data}")
  }
}

impl std::str::FromStr for DisplayFormat {
  type Err = std::io::Error;

  fn from_str(s: &str) -> std::io::Result<Self> {
    match s {
      "yaml" | "yml" => Ok(DisplayFormat::Yaml),
      "toml" => Ok(DisplayFormat::Toml),
      "json" => Ok(DisplayFormat::Json),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Invalid display format {s}"),
      )),
    }
  }
}
