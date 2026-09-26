use chrono::{DateTime, FixedOffset, Utc};
use clap::{Args, Parser};
use std::path::PathBuf;
use tabled::Tabled;

use bollard_next::{
  container::MemoryStatsStats, service::ContainerStateStatusEnum,
};

use nanocld_client::stubs::{
  generic::{GenericClause, GenericFilter},
  process::{Process, ProcessStats},
};

pub struct ProcessArg;

/// `nanocl exec` available options
#[derive(Clone, Parser)]
pub struct ExecOpts {
  /// Run the command in the background
  #[clap(short = 'd', long)]
  pub detach: bool,
  /// Override the key sequence for detaching from an interactive exec
  #[clap(long)]
  pub detach_keys: Option<String>,
  /// Set an environment variable
  #[clap(short = 'e', long = "env")]
  pub env: Vec<String>,
  /// Read environment variables from a file
  #[clap(long = "env-file")]
  pub env_file: Vec<PathBuf>,
  /// Keep standard input open
  #[clap(short = 'i', long)]
  pub interactive: bool,
  /// Give the command extended privileges
  #[clap(long)]
  pub privileged: bool,
  /// Allocate a pseudo-TTY
  #[clap(short = 't', long)]
  pub tty: bool,
  /// User and optional group for the command
  #[clap(short = 'u', long)]
  pub user: Option<String>,
  /// Working directory inside the process
  #[clap(short = 'w', long)]
  pub workdir: Option<String>,
  /// Concrete process name or full Docker ID
  pub process: String,
  /// Command and arguments to execute
  #[clap(required = true, num_args = 1.., trailing_var_arg = true)]
  pub command: Vec<String>,
}

/// `nanocl kill` available options
#[derive(Clone, Parser)]
pub struct KillOpts {
  /// Signal to send to the process
  #[clap(short, long, default_value = "SIGKILL")]
  pub signal: String,
  /// Concrete process name or full Docker ID
  pub process: String,
}

/// `nanocl ps` available options
#[derive(Default, Clone, Args)]
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
  /// Show node names, full image references, and exact creation timestamps
  #[clap(long)]
  pub wide: bool,
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
  /// Name of the instance of the process
  name: String,
  /// Image used by the process
  image: String,
  /// IP address of the process
  ip: String,
  /// Node name
  node: String,
  /// Status of the process
  status: String,
  /// When the process was created
  #[tabled(rename = "CREATED AT")]
  created_at: String,
  #[tabled(skip)]
  age: String,
}

/// A compact row for the process table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ProcessCompactRow {
  name: String,
  image: String,
  ip: String,
  status: String,
  age: String,
}

impl From<ProcessRow> for ProcessCompactRow {
  fn from(row: ProcessRow) -> Self {
    Self {
      name: row.name,
      image: row.image.rsplit('/').next().unwrap_or_default().to_owned(),
      ip: row.ip,
      status: row.status,
      age: row.age,
    }
  }
}

fn format_process_age(
  created_at: Option<&DateTime<FixedOffset>>,
  now: DateTime<Utc>,
) -> String {
  let Some(created_at) = created_at else {
    return "<unknown>".to_owned();
  };
  let seconds = now.signed_duration_since(*created_at).num_seconds().max(0);
  match seconds {
    0..60 => format!("{seconds}s"),
    60..3600 => format!("{}m", seconds / 60),
    3600..86400 => format!("{}h", seconds / 3600),
    _ => format!("{}d", seconds / 86400),
  }
}

/// Convert Process to ProcessRow
impl From<Process> for ProcessRow {
  fn from(process: Process) -> Self {
    let container = process.data;
    let name = container.name.unwrap_or_default().replace('/', "");
    let config = container.config.unwrap_or_default();
    let network = container.network_settings.unwrap_or_default();
    let networks = network.networks.unwrap_or_default();
    let network_mode = container
      .host_config
      .unwrap_or_default()
      .network_mode
      .unwrap_or("nanoclbr0".to_owned());
    let ip_addr = process
      .ip_address
      .filter(|ip| !ip.is_empty())
      .unwrap_or_else(|| match network_mode.as_str() {
        "host" => "<host>".to_owned(),
        "none" => "<none>".to_owned(),
        "bridge" => "<bridge>".to_owned(),
        s if s.starts_with("container:") => "<shared>".to_owned(),
        _ => {
          if let Some(network) = networks.get(&network_mode) {
            let mut ip_addr = network
              .ip_address
              .clone()
              .unwrap_or(network_mode.to_owned());
            if ip_addr.is_empty() {
              "<none>".clone_into(&mut ip_addr);
            }
            ip_addr
          } else {
            format!("<{}>", network_mode)
          }
        }
      });
    let now = chrono::Local::now();
    let created_at = container
      .created
      .as_deref()
      .and_then(|created_at| DateTime::parse_from_rfc3339(created_at).ok());
    let age = format_process_age(created_at.as_ref(), now.with_timezone(&Utc));
    // Show exact creation timestamps in the current timezone in wide output.
    let created_at = created_at
      .map(|created_at| {
        created_at
          .with_timezone(now.offset())
          .format("%Y-%m-%d %H:%M:%S")
          .to_string()
      })
      .unwrap_or_else(|| "<unknown>".to_owned());
    let status = container
      .state
      .unwrap_or_default()
      .status
      .unwrap_or(ContainerStateStatusEnum::EMPTY)
      .to_string();
    Self {
      key: process.key,
      name: name.to_owned(),
      image: config.image.unwrap_or_default(),
      node: process.node_name,
      status,
      ip: ip_addr,
      created_at,
      age,
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

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use chrono::{DateTime, Duration, Utc};
  use clap::Parser;
  use tabled::Tabled;

  use super::{Process, ProcessCompactRow, ProcessRow, format_process_age};
  use crate::models::{Cli, Command};

  fn process_with_network(mode: &str) -> Process {
    serde_json::from_value(serde_json::json!({
      "Key": "process-id",
      "CreatedAt": "2026-09-26T00:00:00",
      "UpdatedAt": "2026-09-26T00:00:00",
      "Name": "process",
      "Kind": "cargo",
      "NodeName": "node",
      "KindKey": "global.process.c",
      "Data": {
        "HostConfig": { "NetworkMode": mode },
        "NetworkSettings": {
          "Networks": {
            "nanoclbr0": { "IPAddress": "10.88.0.12" }
          }
        }
      }
    }))
    .unwrap()
  }

  #[test]
  fn process_row_uses_resolved_shared_ip() {
    let mut process = process_with_network("container:network-owner");
    process.ip_address = Some("10.88.0.12".to_owned());

    assert_eq!(ProcessRow::from(process).ip, "10.88.0.12");
  }

  #[test]
  fn process_row_keeps_network_fallbacks_without_resolved_ip() {
    for (mode, expected) in [
      ("container:network-owner", "<shared>"),
      ("nanoclbr0", "10.88.0.12"),
      ("host", "<host>"),
      ("none", "<none>"),
      ("bridge", "<bridge>"),
    ] {
      for ip_address in [None, Some(String::new())] {
        let mut process = process_with_network(mode);
        process.ip_address = ip_address;

        assert_eq!(ProcessRow::from(process).ip, expected, "mode {mode}");
      }
    }
  }

  #[test]
  fn ps_compact_row_preserves_process_name_and_image_tag_or_digest() {
    for (image, expected) in [
      ("ghcr.io/next-hat/metrsd:0.5.8", "metrsd:0.5.8"),
      ("registry.local:5000/team/nested/app:dev", "app:dev"),
      (
        "docker.io/cockroachdb/cockroach:v25.4.13",
        "cockroach:v25.4.13",
      ),
      ("alpine", "alpine"),
      ("alpine:latest", "alpine:latest"),
      (
        "ghcr.io/next-hat/app@sha256:0123456789abcdef",
        "app@sha256:0123456789abcdef",
      ),
      (
        "ghcr.io/next-hat/app:dev@sha256:0123456789abcdef",
        "app:dev@sha256:0123456789abcdef",
      ),
    ] {
      let mut process = process_with_network("nanoclbr0");
      let name = "global.deploy-example-r0-app-yY03yU.c";
      process.data.name = Some(format!("/{name}"));
      process
        .data
        .config
        .get_or_insert_with(Default::default)
        .image = Some(image.to_owned());
      process.data.created = Some("2026-09-26T00:00:00Z".to_owned());
      let wide = ProcessRow::from(process);
      assert_eq!(wide.name, name);
      assert_eq!(wide.image, image);
      assert_eq!(wide.node, "node");
      let age = wide.age.clone();
      let compact = ProcessCompactRow::from(wide);
      assert_eq!(compact.name, name);
      assert_eq!(compact.image, expected);
      assert_eq!(compact.ip, "10.88.0.12");
      assert_eq!(compact.age, age);
    }
    assert_eq!(
      ProcessCompactRow::headers(),
      ["NAME", "IMAGE", "IP", "STATUS", "AGE"]
    );
    assert_eq!(
      ProcessRow::headers(),
      ["NAME", "IMAGE", "IP", "NODE", "STATUS", "CREATED AT"]
    );
  }

  #[test]
  fn ps_age_uses_compact_units_at_boundaries() {
    let now = DateTime::parse_from_rfc3339("2026-09-26T12:00:00Z").unwrap();
    for (seconds, expected) in [
      (0, "0s"),
      (59, "59s"),
      (60, "1m"),
      (3599, "59m"),
      (3600, "1h"),
      (86399, "23h"),
      (86400, "1d"),
      (172800, "2d"),
    ] {
      let created_at = now - Duration::seconds(seconds);
      assert_eq!(
        format_process_age(Some(&created_at), now.with_timezone(&Utc)),
        expected
      );
    }
  }

  #[test]
  fn ps_age_handles_timezones_and_future_creation() {
    let now = DateTime::parse_from_rfc3339("2026-09-26T12:00:00Z")
      .unwrap()
      .with_timezone(&Utc);
    for (created_at, expected) in [
      ("2026-09-26T13:00:00+02:00", "1h"),
      ("2026-09-26T06:30:00-05:00", "30m"),
      ("2026-09-26T12:00:01Z", "0s"),
    ] {
      let created_at = DateTime::parse_from_rfc3339(created_at).unwrap();
      assert_eq!(format_process_age(Some(&created_at), now), expected);
    }
  }

  #[test]
  fn ps_rows_show_unknown_for_missing_or_invalid_creation() {
    for created_at in [None, Some(""), Some("invalid")] {
      let mut process = process_with_network("nanoclbr0");
      process.data.created = created_at.map(str::to_owned);
      let wide = ProcessRow::from(process);
      assert_eq!(wide.created_at, "<unknown>");
      assert_eq!(wide.age, "<unknown>");
      assert_eq!(ProcessCompactRow::from(wide).age, "<unknown>");
    }
  }

  #[test]
  fn ps_wide_parses_with_filters_and_quiet() {
    let cli = Cli::try_parse_from([
      "nanocl",
      "ps",
      "--wide",
      "--quiet",
      "--namespace",
      "global",
      "--kind",
      "cargo",
      "--all",
      "--limit",
      "5",
      "--offset",
      "2",
      "--filters",
      "name=app",
    ])
    .expect("wide ps command must parse with existing options");
    let Command::Ps(options) = cli.command else {
      panic!("expected ps command");
    };
    assert!(options.quiet);
    assert_eq!(options.limit, Some(5));
    assert_eq!(options.offset, Some(2));
    assert_eq!(options.filters, Some(vec!["name=app".to_owned()]));
    let filter = options.others.unwrap();
    assert!(filter.wide);
    assert!(filter.all);
    assert_eq!(filter.namespace.as_deref(), Some("global"));
    assert_eq!(filter.kind.as_deref(), Some("cargo"));

    let cli = Cli::try_parse_from(["nanocl", "ps"])
      .expect("default ps command must parse");
    let Command::Ps(options) = cli.command else {
      panic!("expected ps command");
    };
    assert!(!options.others.unwrap_or_default().wide);
    assert!(!options.quiet);
  }

  #[test]
  fn exec_command_parses_options_and_preserves_command_arguments() {
    let cli = Cli::try_parse_from([
      "nanocl",
      "exec",
      "-it",
      "--env-file",
      "first.env",
      "-e",
      "FOO=bar",
      "process-id",
      "sh",
      "-c",
      "echo hello",
      "--privileged",
    ])
    .expect("exec command must parse");
    let Command::Exec(options) = cli.command else {
      panic!("expected exec command");
    };
    assert!(options.interactive);
    assert!(options.tty);
    assert_eq!(options.env_file, [PathBuf::from("first.env")]);
    assert_eq!(options.env, ["FOO=bar"]);
    assert_eq!(options.process, "process-id");
    assert_eq!(options.command, ["sh", "-c", "echo hello", "--privileged"]);

    let cli = Cli::try_parse_from([
      "nanocl",
      "exec",
      "process-id",
      "--",
      "sh",
      "-c",
      "echo hello",
    ])
    .expect("exec command with separator must parse");
    let Command::Exec(options) = cli.command else {
      panic!("expected exec command");
    };
    assert_eq!(options.command, ["sh", "-c", "echo hello"]);
  }
}
