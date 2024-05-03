use clap::Args;
use tabled::Tabled;
use chrono::DateTime;

use bollard_next::{container::MemoryStatsStats, service::ContainerStateStatusEnum};

use nanocld_client::stubs::{
  generic::{GenericClause, GenericFilter},
  process::{Process, ProcessStats},
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
      .unwrap_or("Unknown".to_owned());
    let namespace = if kind.as_str() != "job" {
      names.next().unwrap_or("<none>")
    } else {
      "<none>"
    };
    let network = container.network_settings.unwrap_or_default();
    let networks = network.networks.unwrap_or_default();
    let mut ip_addr = if let Some(network) = networks.get(namespace) {
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
    if ip_addr.is_empty() {
      "<none>".clone_into(&mut ip_addr);
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
      ip: ip_addr,
      created_at,
    }
  }
}

/// A row of the cargo stats table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ProcessStatsRow {
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

impl From<ProcessStats> for ProcessStatsRow {
  fn from(process_stats: ProcessStats) -> Self {
    let stats = process_stats.stats;
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
      let total_read = match io_service_bytes_recursive.first() {
        Some(val) => val.value,
        None => 0,
      };
      let total_write = match io_service_bytes_recursive.get(1) {
        Some(val) => val.value,
        None => 0,
      };
      (total_read as f64, total_write as f64)
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
