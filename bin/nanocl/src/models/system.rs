use clap::Parser;
use tabled::Tabled;
use chrono::DateTime;

use bollard_next::service::ContainerStateStatusEnum;

use nanocld_client::stubs::system::ProccessQuery;
use nanocld_client::stubs::node::NodeContainerSummary;
use nanocld_client::stubs::http_metric::HttpMetricListQuery;

/// ## SystemArg
///
/// `nanocl system` available arguments
///
#[derive(Clone, Parser)]
pub struct SystemArg {
  /// Command to run
  #[clap(subcommand)]
  pub command: SystemCommand,
}

/// ## SystemCommand
///
/// `nanocl system` available commands
///
#[derive(Clone, Parser)]
pub enum SystemCommand {
  /// System HTTP metrics information
  Http(SystemHttpArg),
}

/// ## SystemHttpArg
///
/// `nanocl system http` available arguments
///
#[derive(Clone, Parser)]
pub struct SystemHttpArg {
  /// Command to run
  #[clap(subcommand)]
  pub command: SystemHttpCommand,
}

/// ## SystemHttpCommand
///
/// `nanocl system http` available commands
///
#[derive(Clone, Parser)]
pub enum SystemHttpCommand {
  /// Show HTTP metrics information
  Logs(SystemHttpLogsOpts),
}

/// ## SystemHttpLogsOpts
///
/// `nanocl system http logs` available options
///
#[derive(Clone, Parser)]
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
/// `nanocl ps` available options
///
#[derive(Clone, Parser)]
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
      namespace: opts.namespace,
    }
  }
}

/// ## ProcessRow
///
/// A row for the process table
///
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ProcessRow {
  /// Kind of instance cargo or vm
  kind: String,
  /// Node name
  node: String,
  /// Name of the instance of the cargo or the vm
  name: String,
  /// Namespace of the cargo or the vm
  namespace: String,
  /// Image used by the cargo or the vm
  image: String,
  /// Status of the cargo or the vm
  status: String,
  /// IP address of the cargo or the vm
  ip: String,
  /// When the cargo or the vm was created
  #[tabled(rename = "CREATED AT")]
  created_at: String,
}

/// Convert NodeContainerSummary to ProcessRow
impl From<NodeContainerSummary> for ProcessRow {
  fn from(summary: NodeContainerSummary) -> Self {
    let container = summary.container;
    let name = container.name.unwrap_or_default().replace('/', "");
    let mut names = name.split('.');
    let name = names.next().unwrap_or(&name);
    let namespace = names.next().unwrap_or("None");
    let network = container.network_settings.unwrap_or_default();
    let networks = network.networks.unwrap_or_default();
    let mut ipaddr = String::default();
    if let Some(network) = networks.get(namespace) {
      ipaddr = network.ip_address.clone().unwrap_or_default();
    }
    let config = container.config.unwrap_or_default();
    let kind = config
      .labels
      .unwrap_or_default()
      .get("io.nanocl.kind")
      .cloned()
      .unwrap_or("Unknow".to_owned());
    // Convert the created_at and updated_at to the current timezone
    let created_at = container.created.unwrap_or_default();
    let binding = chrono::Local::now();
    let tz = binding.offset();
    let created_at = DateTime::parse_from_rfc3339(&created_at)
      .unwrap_or_default()
      .with_timezone(tz)
      .format("%Y-%m-%d %H:%M:%S")
      .to_string();
    let status = container
      .state
      .unwrap_or_default()
      .status
      .unwrap_or(ContainerStateStatusEnum::EMPTY)
      .to_string();
    Self {
      node: summary.node,
      kind,
      name: name.to_owned(),
      namespace: namespace.to_owned(),
      image: config.image.unwrap_or_default(),
      status,
      ip: ipaddr,
      created_at,
    }
  }
}
