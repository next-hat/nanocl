use chrono::TimeZone;
use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::metric::Metric;

use super::GenericListOpts;

#[derive(Clone, Parser)]
pub struct MetricArg {
  #[clap(subcommand)]
  pub command: MetricCommand,
}

/// metric available commands
#[derive(Clone, Subcommand)]
pub enum MetricCommand {
  /// List existing job
  #[clap(alias("ls"))]
  List(GenericListOpts),
}

#[derive(Clone, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct MetricRow {
  pub key: String,
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
  pub node: String,
  pub kind: String,
  pub note: String,
}

impl From<Metric> for MetricRow {
  fn from(metric: Metric) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(metric.created_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: metric
        .key
        .to_string()
        .split('-')
        .last()
        .unwrap_or("<error>")
        .to_string(),
      created_at: created_at.to_string(),
      kind: metric.kind,
      node: metric.node_name,
      note: metric.note.unwrap_or("<none>".to_owned()),
    }
  }
}
