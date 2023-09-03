use clap::Parser;
use tabled::Tabled;
use chrono::TimeZone;

use nanocld_client::stubs::system::ProccessQuery;
use nanocld_client::stubs::node::NodeContainerSummary;
use nanocld_client::stubs::http_metric::HttpMetricListQuery;

/// ## SystemOpts
///
/// System command options
///
#[derive(Clone, Debug, Parser)]
pub struct SystemOpts {
  /// Command to run
  #[clap(subcommand)]
  pub command: SystemCommand,
}

/// ## SystemCommand
///
/// Available system commands
///
#[derive(Clone, Debug, Parser)]
pub enum SystemCommand {
  /// System HTTP metrics information
  Http(SystemHttpOpts),
}

/// ## SystemHttpOpts
///
/// System HTTP metrics options
///
#[derive(Clone, Debug, Parser)]
pub struct SystemHttpOpts {
  /// Command to run
  #[clap(subcommand)]
  pub command: SystemHttpCommand,
}

/// ## SystemHttpCommand
///
/// Available system http commands
///
#[derive(Clone, Debug, Parser)]
pub enum SystemHttpCommand {
  /// Show HTTP metrics information
  Logs(SystemHttpLogsOpts),
}

/// ## SystemHttpLogsOpts
///
/// System HTTP logs options
///
#[derive(Clone, Debug, Parser)]
pub struct SystemHttpLogsOpts {
  // #[clap(long, short)]
  // pub follow: bool,
  /// Limit the number of results
  #[clap(long, short)]
  pub limit: Option<i64>,
  /// Offset the number of results
  #[clap(long, short)]
  pub offset: Option<i64>,
}

/// Convert SystemHttpLogsOpts to HttpMetricListQuery
impl From<SystemHttpLogsOpts> for HttpMetricListQuery {
  fn from(opts: SystemHttpLogsOpts) -> Self {
    Self {
      limit: opts.limit,
      offset: opts.offset,
    }
  }
}

/// ## ProcessOpts
///
/// Process command options
///
#[derive(Clone, Debug, Parser)]
pub struct ProcessOpts {
  /// Return containers for all nodes by default only the current node
  #[clap(long, short)]
  pub all: bool,
  /// Return this number of most recently created containers, including non-running ones
  #[clap(long)]
  pub last: Option<isize>,
  /// Return the size of container as fields `SizeRw` and `SizeRootFs`
  #[clap(long, short)]
  pub size: bool,
  /// Show all containers running for the given namespace
  #[clap(long, short)]
  pub namespace: Option<String>,
}

/// Convert ProcessOpts to ProccessQuery
impl From<ProcessOpts> for ProccessQuery {
  fn from(opts: ProcessOpts) -> Self {
    Self {
      all: opts.all,
      last: opts.last,
      size: opts.size,
      namespace: opts.namespace,
    }
  }
}

/// ## ProcessRow
///
/// A row for the process table
///
#[derive(Tabled)]
pub struct ProcessRow {
  /// Node name
  node: String,
  /// Name of the instance of the cargo or the vm
  name: String,
  /// Namespace of the cargo or the vm
  namespace: String,
  /// Kind of instance cargo or vm
  kind: String,
  /// Image used by the cargo or the vm
  image: String,
  /// Status of the cargo or the vm
  status: String,
  /// IP address of the cargo or the vm
  ip_address: String,
  /// When the cargo or the vm was created
  created: String,
}

/// Convert NodeContainerSummary to ProcessRow
impl From<NodeContainerSummary> for ProcessRow {
  fn from(summary: NodeContainerSummary) -> Self {
    let container = summary.container;
    let names = container.names.unwrap_or_default();
    let binding = String::default();
    let name = names.first().unwrap_or(&binding).replace('/', "");
    let mut names = name.split('.');
    let name = names.next().unwrap_or(&name);
    let namespace = names.next().unwrap_or("Unknown");
    let kind = match names.next() {
      Some(kind) => match kind {
        "c" => "cargo".to_owned(),
        "v" => "vm".to_owned(),
        _ => "Undefined".to_owned(),
      },
      None => "Undefined".to_owned(),
    };
    let network = container.network_settings.unwrap_or_default();
    let networks = network.networks.unwrap_or_default();
    let mut ipaddr = String::default();
    if let Some(network) = networks.get(namespace) {
      ipaddr = network.ip_address.clone().unwrap_or_default();
    }
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(container.created.unwrap_or_default(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      node: summary.node,
      kind,
      name: name.to_owned(),
      namespace: namespace.to_owned(),
      image: container.image.unwrap_or_default(),
      status: container.status.unwrap_or_default(),
      ip_address: ipaddr,
      created: format!("{created_at}"),
    }
  }
}
