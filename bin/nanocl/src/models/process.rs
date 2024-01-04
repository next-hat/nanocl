use clap::Args;
use tabled::Tabled;
use chrono::DateTime;

use bollard_next::service::ContainerStateStatusEnum;

use nanocld_client::stubs::{
  process::Process,
  generic::{GenericFilter, GenericClause},
};

pub struct ProcessArg;

/// `nanocl ps` available options
#[derive(Clone, Args)]
pub struct ProcessFilter {
  /// Show all processes for the given namespace
  #[clap(long, short)]
  pub namespace: Option<String>,
  /// Show all processes for the given kind
  #[clap(long, short)]
  pub kind: Option<String>,
  // Show all processes (default shows just running)
  #[clap(long, short)]
  pub all: bool,
}

impl From<ProcessFilter> for GenericFilter {
  fn from(filter: ProcessFilter) -> Self {
    let mut gen_filter = GenericFilter::new();
    if !filter.all {
      gen_filter = gen_filter.r#where(
        "data",
        GenericClause::Contains(serde_json::json!({
          "State": {
            "Status": "running"
          }
        })),
      );
    }
    if let Some(kind) = &filter.kind {
      gen_filter = gen_filter.r#where("kind", GenericClause::Eq(kind.clone()));
    }
    if let Some(namespace) = &filter.namespace {
      gen_filter = gen_filter.r#where(
        "data",
        GenericClause::Contains(serde_json::json!({
          "Config": {
            "Labels": {
              "io.nanocl.n": namespace
            }
          }
        })),
      );
    }
    gen_filter
  }
}

/// A row for the process table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ProcessRow {
  #[tabled(skip)]
  pub key: String,
  /// Namespace of the cargo or the vm
  namespace: String,
  /// Kind of instance cargo or vm
  kind: String,
  /// Name of the instance of the cargo or the vm
  name: String,
  /// Image used by the cargo or the vm
  image: String,
  /// IP address of the cargo or the vm
  ip: String,
  /// Node name
  node: String,
  /// Status of the cargo or the vm
  status: String,
  /// When the cargo or the vm was created
  #[tabled(rename = "CREATED AT")]
  created_at: String,
}

/// Convert Process to ProcessRow
impl From<Process> for ProcessRow {
  fn from(process: Process) -> Self {
    let container = process.data;
    let name = container.name.unwrap_or_default().replace('/', "");
    let mut names = name.split('.');
    let name = names.next().unwrap_or(&name);
    let config = container.config.unwrap_or_default();
    let kind = config
      .labels
      .unwrap_or_default()
      .get("io.nanocl.kind")
      .cloned()
      .unwrap_or("Unknow".to_owned());
    let namespace = if kind.as_str() != "job" {
      names.next().unwrap_or("<none>")
    } else {
      "<none>"
    };
    let network = container.network_settings.unwrap_or_default();
    let networks = network.networks.unwrap_or_default();
    let mut ipaddr = if let Some(network) = networks.get(namespace) {
      network.ip_address.clone().unwrap_or("<none>".to_owned())
    } else {
      format!(
        "<{}>",
        container
          .host_config
          .unwrap_or_default()
          .network_mode
          .unwrap_or("<none>".to_owned())
      )
    };
    if ipaddr.is_empty() {
      ipaddr = "<none>".to_owned();
    }
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
      key: process.key,
      node: process.node_key,
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
