use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use nanocld_client::stubs::metric::Metric;

use super::{GenericInspectOpts, GenericListOpts, format_age};

#[derive(Clone, Parser)]
pub struct MetricArg {
  #[clap(subcommand)]
  pub command: MetricCommand,
}

/// metric available commands
#[derive(Clone, Subcommand)]
pub enum MetricCommand {
  /// List existing metrics
  #[clap(alias("ls"))]
  List(GenericListOpts),
  /// Inspect a metric
  Inspect(GenericInspectOpts),
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
  #[tabled(skip)]
  pub age: String,
}

#[derive(Clone, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct MetricCompactRow {
  pub key: String,
  pub age: String,
  pub kind: String,
  pub note: String,
}

impl From<MetricRow> for MetricCompactRow {
  fn from(row: MetricRow) -> Self {
    Self {
      key: row.key,
      age: row.age,
      kind: row.kind,
      note: row.note,
    }
  }
}

impl From<Metric> for MetricRow {
  fn from(metric: Metric) -> Self {
    let binding = chrono::Local::now();
    let age = format_age(
      Some(&metric.created_at.and_utc()),
      binding.with_timezone(&chrono::Utc),
    );
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(metric.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: metric.key.to_string(),
      created_at: created_at.to_string(),
      kind: metric.kind,
      node: metric.node_name,
      note: metric.note.unwrap_or("<none>".to_owned()),
      age,
    }
  }
}

#[cfg(test)]
mod tests {
  use chrono::{Duration, Utc};

  use crate::commands::GenericCommandLs;

  use super::{
    GenericListOpts, Metric, MetricArg, MetricCompactRow, MetricRow,
  };

  fn metric() -> Metric {
    let created_at = (Utc::now() - Duration::hours(73)).naive_utc();
    Metric {
      key: uuid::Uuid::from_u128(2),
      created_at,
      expires_at: created_at + Duration::days(7),
      node_name: "worker-a".to_owned(),
      kind: "Process".to_owned(),
      data: serde_json::json!({"cpu": 0.5}),
      note: Some("Collected api.global CPU usage".to_owned()),
    }
  }

  #[test]
  fn metric_list_age_uses_api_creation_time() {
    let wide = MetricRow::from(metric());
    assert_eq!(wide.age, "3d");
    let compact = MetricCompactRow::from(wide.clone());
    assert_eq!(compact.age, wide.age);
    assert_eq!(compact.key, wide.key);
    assert_eq!(compact.note, wide.note);
    assert_eq!(compact.kind, wide.kind);
  }

  #[test]
  fn metric_list_compact_wide_and_quiet_preserve_content() {
    let row = MetricRow::from(metric());
    let mut opts: GenericListOpts = GenericListOpts::default();
    let compact = MetricArg::render_list(&opts, vec![row.clone()]);
    for header in ["KEY", "AGE", "KIND", "NOTE"] {
      assert!(compact.contains(header));
    }
    assert!(!compact.contains("NODE"));
    assert!(!compact.contains("CREATED AT"));
    assert!(!compact.contains(&row.node));
    assert!(!compact.contains(&row.created_at));
    assert!(compact.contains(&row.key));
    assert!(compact.contains(&row.note));
    assert!(compact.contains(&row.kind));

    opts.wide = true;
    let wide = MetricArg::render_list(&opts, vec![row.clone()]);
    assert!(wide.contains("NODE"));
    assert!(wide.contains("CREATED AT"));
    assert!(!wide.contains("AGE"));
    assert!(wide.contains(&row.node));
    assert!(wide.contains(&row.created_at));
    assert!(wide.contains(&row.key));
    assert!(wide.contains(&row.note));

    opts.quiet = true;
    assert_eq!(MetricArg::render_list(&opts, vec![row.clone()]), row.key);
  }
}
