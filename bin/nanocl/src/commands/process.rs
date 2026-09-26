use tabled::Table;
use tabled::settings::object::Segment;
use tabled::settings::{Alignment, Modify, Padding, Style};

use nanocl_error::io::IoResult;
use nanocld_client::stubs::process::{
  Process, ProcessKillOptions, ProcessLogQuery,
};

use crate::{
  config::CliConfig,
  models::{
    GenericInspectOpts, GenericListOpts, KillOpts, LogsOpts, ProcessArg,
    ProcessCompactRow, ProcessFilter, ProcessRow,
  },
  utils,
};

use super::{GenericCommand, GenericCommandInspect, GenericCommandLs};

impl GenericCommandInspect for ProcessArg {
  type ApiItem = Process;
}

impl GenericCommand for ProcessArg {
  fn object_name() -> &'static str {
    "processes"
  }
}

impl GenericCommandLs for ProcessArg {
  type Item = ProcessRow;
  type Args = ProcessArg;
  type ApiItem = Process;

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }
}

/// Get logs for a process by name or id
pub async fn logs_process(
  cli_conf: &CliConfig,
  opts: &LogsOpts,
) -> IoResult<()> {
  let query: ProcessLogQuery = opts.clone().into();
  let mut streams = Vec::with_capacity(opts.names.len());
  for name in &opts.names {
    match cli_conf.client.logs_process(name, Some(&query)).await {
      Ok(stream) => streams.push(stream),
      Err(err) => eprintln!("WARN: cannot stream logs for {name}: {err}"),
    }
  }
  utils::print::logs_process_streams(streams).await?;
  Ok(())
}

/// Inspect a process by it's name
pub async fn inspect_process(
  cli_conf: &CliConfig,
  opts: &GenericInspectOpts,
) -> IoResult<()> {
  ProcessArg::exec_inspect(cli_conf, opts).await?;
  Ok(())
}

/// Send a signal to a concrete process by its name or full Docker ID
pub async fn kill_process(
  cli_conf: &CliConfig,
  opts: &KillOpts,
) -> IoResult<()> {
  cli_conf
    .client
    .kill_process(
      &opts.process,
      &ProcessKillOptions {
        signal: opts.signal.clone(),
      },
    )
    .await?;
  Ok(())
}

pub async fn exec_process(
  cli_conf: &CliConfig,
  opts: &GenericListOpts<ProcessFilter>,
) -> IoResult<()> {
  let filter = ProcessArg::gen_default_filter(&ProcessArg, opts);
  let rows = cli_conf
    .client
    .list_process(Some(&filter))
    .await?
    .into_iter()
    .map(ProcessRow::from)
    .collect();
  let output = render_process_list(rows, opts);
  if !output.is_empty() {
    println!("{output}");
  }
  Ok(())
}

fn render_process_list(
  rows: Vec<ProcessRow>,
  opts: &GenericListOpts<ProcessFilter>,
) -> String {
  if opts.quiet {
    return rows
      .iter()
      .map(ProcessArg::get_key)
      .collect::<Vec<_>>()
      .join("\n");
  }
  let wide = opts.others.as_ref().is_some_and(|filter| filter.wide);
  let mut table = if wide {
    Table::new(rows)
  } else {
    Table::new(rows.into_iter().map(ProcessCompactRow::from))
  };
  table
    .with(Style::empty())
    .with(
      Modify::new(Segment::all())
        .with(Padding::new(0, 2, 0, 0))
        .with(Alignment::left()),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn process_row(key: &str) -> ProcessRow {
    let process: Process = serde_json::from_value(serde_json::json!({
      "Key": key,
      "CreatedAt": "2026-09-26T10:00:00",
      "UpdatedAt": "2026-09-26T10:00:00",
      "Name": "global.deploy-example-r0-app-yY03yU.c",
      "Kind": "cargo",
      "NodeName": "nanocl.internal",
      "KindKey": "global.deploy-example",
      "IpAddress": "10.20.1.5",
      "Data": {
        "Name": "/global.deploy-example-r0-app-yY03yU.c",
        "Created": "2026-09-26T10:00:00Z",
        "Config": { "Image": "ghcr.io/next-hat/nanocl-get-started:latest" },
        "State": { "Status": "running" },
        "HostConfig": { "NetworkMode": "container:owner" }
      }
    }))
    .unwrap();
    process.into()
  }

  #[test]
  fn process_table_switches_between_compact_and_wide_details() {
    let mut opts = GenericListOpts::<ProcessFilter>::default();
    let compact = render_process_list(vec![process_row("full-id")], &opts);
    assert_eq!(
      compact
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>(),
      ["NAME", "IMAGE", "IP", "STATUS", "AGE"]
    );
    assert!(compact.contains("global.deploy-example-r0-app-yY03yU.c"));
    assert!(compact.contains("nanocl-get-started:latest"));
    assert!(compact.contains("10.20.1.5"));
    assert!(!compact.contains("ghcr.io/next-hat/"));
    assert!(!compact.contains("nanocl.internal"));

    opts.others = Some(ProcessFilter {
      wide: true,
      ..Default::default()
    });
    let wide = render_process_list(vec![process_row("full-id")], &opts);
    assert_eq!(
      wide
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>(),
      ["NAME", "IMAGE", "IP", "NODE", "STATUS", "CREATED", "AT"]
    );
    assert!(wide.contains("global.deploy-example-r0-app-yY03yU.c"));
    assert!(wide.contains("ghcr.io/next-hat/nanocl-get-started:latest"));
    assert!(wide.contains("nanocl.internal"));
    assert!(
      compact.lines().next().unwrap().len()
        < wide.lines().next().unwrap().len()
    );
  }

  #[test]
  fn process_quiet_output_keeps_full_keys_in_both_layouts() {
    for wide in [false, true] {
      let opts = GenericListOpts {
        quiet: true,
        others: Some(ProcessFilter {
          wide,
          ..Default::default()
        }),
        ..Default::default()
      };
      assert_eq!(
        render_process_list(
          vec![process_row("first-full-id"), process_row("second-full-id")],
          &opts
        ),
        "first-full-id\nsecond-full-id"
      );
      assert!(render_process_list(Vec::new(), &opts).is_empty());
    }
  }

  #[test]
  fn empty_process_tables_keep_their_headers() {
    for (wide, expected) in [
      (false, vec!["NAME", "IMAGE", "IP", "STATUS", "AGE"]),
      (
        true,
        vec!["NAME", "IMAGE", "IP", "NODE", "STATUS", "CREATED", "AT"],
      ),
    ] {
      let opts = GenericListOpts {
        others: Some(ProcessFilter {
          wide,
          ..Default::default()
        }),
        ..Default::default()
      };
      let rendered = render_process_list(Vec::new(), &opts);
      assert_eq!(rendered.split_whitespace().collect::<Vec<_>>(), expected);
    }
  }
}
