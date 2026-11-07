//! Read-only, Statefile-scoped process resource usage.

use std::{
  collections::{BTreeMap, BTreeSet},
  io::{self, IsTerminal, Write},
  time::Duration,
};

use dialoguer::console::measure_text_width;
use futures::{StreamExt, stream};
use nanocl_error::{
  http_client::HttpClientError,
  io::{IoError, IoResult},
};
use nanocld_client::stubs::{
  process::{ProcessStats, ProcessStatsQuery},
  statefile::Statefile,
};
use tabled::{
  Table,
  settings::{Alignment, Modify, Padding, Style, object::Segment},
};

use crate::{
  config::CliConfig,
  models::{ProcessStatsCompactRow, ProcessStatsRow, StateRef, StateStatsOpts},
  utils::{
    print::{render_table, terminal_width, wrap_lines},
    process::resource_key,
  },
};

use super::{
  ArgParseMode, parse_build_args, parse_state_file_recurr, read_state_file,
};

fn input_error(stage: &str, err: IoError) -> IoError {
  IoError::with_context(
    format!("State stats: {stage}"),
    io::Error::new(
      err.inner.kind(),
      "failed; details hidden to protect secret values",
    ),
  )
}

fn read_error(kind: &str, key: &str, err: HttpClientError) -> IoError {
  let message = match err {
    HttpClientError::HttpError(err) => {
      format!("daemon read failed (HTTP {})", err.status.as_u16())
    }
    HttpClientError::IoError(_) => "daemon read failed".into(),
  };
  IoError::other(format!("State stats: {kind} {key}"), message)
}

fn targets(
  states: &[StateRef<Statefile>],
) -> IoResult<Vec<(&'static str, String)>> {
  let mut seen = BTreeSet::new();
  let mut targets = Vec::new();
  for state in states {
    let namespace = state.data.namespace.as_deref().unwrap_or("global");
    let mut insert = |kind, key: String| {
      if seen.insert((kind, key.clone())) {
        targets.push((kind, key));
      }
    };
    for cargo in state.data.cargoes.iter().flatten() {
      insert("cargo", resource_key(&cargo.name, namespace)?);
    }
    for vm in state.data.virtual_machines.iter().flatten() {
      insert("vm", resource_key(&vm.name, namespace)?);
    }
    for job in state.data.jobs.iter().flatten() {
      insert("job", job.name.clone());
    }
  }
  Ok(targets)
}

fn render_stats(
  stats: &BTreeMap<String, ProcessStats>,
  wide: bool,
  width: usize,
) -> String {
  if stats.is_empty() {
    return if wide {
      "No process stats available.".into()
    } else {
      wrap_lines("No process stats available.", width, 0)
    };
  }
  let rows = stats.values().cloned().map(ProcessStatsRow::from);
  if wide {
    return render_table(rows);
  }
  let rows = rows
    .map(ProcessStatsCompactRow::from)
    .map(|mut row| {
      // Avoid control sequences in process names changing a watch display.
      row.process = row
        .process
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
      row
    })
    .collect::<Vec<_>>();
  let table = Table::new(&rows)
    .with(Style::empty())
    .with(
      Modify::new(Segment::all())
        .with(Padding::new(0, 2, 0, 0))
        .with(Alignment::left()),
    )
    .to_string();
  let table = table
    .lines()
    .map(str::trim_end)
    .collect::<Vec<_>>()
    .join("\n");
  if table.lines().all(|line| measure_text_width(line) <= width) {
    return table;
  }
  // Long identities stay complete, without making every other row wider.
  rows
    .iter()
    .map(|row| {
      format!(
        "{}\n{}",
        wrap_lines(&row.process, width, 0),
        wrap_lines(
          &format!(
            "CPU {}  MEM {}  MEM% {}",
            row.cpu, row.memory, row.memory_percent
          ),
          width,
          2,
        ),
      )
    })
    .collect::<Vec<_>>()
    .join("\n")
}

fn print_stats(
  stats: &BTreeMap<String, ProcessStats>,
  clear: bool,
  wide: bool,
) -> IoResult<()> {
  let mut stdout = io::stdout().lock();
  if clear {
    stdout.write_all(b"\x1b[2J\x1b[H")?;
  }
  writeln!(stdout, "{}", render_stats(stats, wide, terminal_width()))?;
  stdout.flush()?;
  Ok(())
}

pub(super) async fn exec_state_stats(
  cli_conf: &CliConfig,
  opts: &StateStatsOpts,
) -> IoResult<()> {
  let state =
    read_state_file(&opts.source, &cli_conf.user_config.display_format)
      .await
      .map_err(|err| input_error("read Statefile", err))?;
  let args =
    parse_build_args(&state.data, ArgParseMode::Stats, &opts.args, true)
      .map_err(|err| input_error("Statefile arguments", err))?;
  let states = parse_state_file_recurr(cli_conf, &state, &args, true)
    .await
    .map_err(|err| input_error("render Statefile", err))?;
  let targets =
    targets(&states).map_err(|err| input_error("declarations", err))?;
  if targets.is_empty() {
    println!("No cargoes, VMs, or jobs declared.");
    return Ok(());
  }
  let query = ProcessStatsQuery {
    stream: Some(!opts.no_stream),
    // Wait for two samples so CPU deltas are available in snapshot mode.
    one_shot: Some(false),
  };
  let streams = stream::iter(targets.into_iter().map(|(kind, key)| {
    let query = query.clone();
    async move {
      let result = ntex::time::timeout(
        Duration::from_secs(10),
        cli_conf.client.stats_processes(kind, &key, Some(&query)),
      )
      .await;
      match result {
        Ok(Ok(stream)) => stream
          .map(move |result| {
            result.map_err(|err| read_error(kind, &key, err.into()))
          })
          .boxed_local(),
        Ok(Err(err)) => {
          stream::once(futures::future::ready(Err(read_error(kind, &key, err))))
            .boxed_local()
        }
        Err(_) => stream::once(futures::future::ready(Err(IoError::other(
          format!("State stats: {kind} {key}"),
          "daemon read timed out".to_owned(),
        ))))
        .boxed_local(),
      }
    }
  }))
  .buffer_unordered(8)
  .flatten_unordered(None);
  futures::pin_mut!(streams);
  let clear = !opts.no_stream
    && io::stdout().is_terminal()
    && std::env::var("TERM").as_deref() != Ok("dumb");
  let mut stats = BTreeMap::new();
  let mut read_failed = false;
  while let Some(result) = streams.next().await {
    match result {
      Ok(sample) => {
        stats.insert(sample.name.clone(), sample);
        if !opts.no_stream {
          print_stats(&stats, clear, opts.wide)?;
        }
      }
      Err(err) => {
        eprintln!("{err}");
        read_failed = true;
      }
    }
  }
  if opts.no_stream || stats.is_empty() {
    print_stats(&stats, false, opts.wide)?;
  }
  if read_failed {
    return Err(IoError::other("State stats", "some stats reads failed"));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::{
    cargo_spec::CargoSpec, job::JobPartial, resource::ResourcePartial,
    vm_spec::VmSpecPartial,
  };

  use super::*;

  fn state_ref(namespace: Option<&str>) -> StateRef<Statefile> {
    let mut data: Statefile =
      serde_json::from_value(serde_json::json!({"ApiVersion": "v0.19"}))
        .unwrap();
    data.namespace = namespace.map(str::to_owned);
    data.cargoes = Some(vec![CargoSpec {
      name: "app".into(),
      ..Default::default()
    }]);
    data.virtual_machines = Some(vec![VmSpecPartial {
      name: "app".into(),
      ..Default::default()
    }]);
    data.jobs = Some(vec![JobPartial {
      name: "migrate".into(),
      ..Default::default()
    }]);
    data.resources = Some(vec![ResourcePartial {
      name: "route".into(),
      kind: "ProxyRule".into(),
      data: serde_json::json!({}),
      metadata: None,
    }]);
    StateRef {
      raw: String::new(),
      format: Default::default(),
      data,
      root: Default::default(),
      location: "Statefile.yml".into(),
    }
  }

  #[test]
  fn stats_targets_scope_kinds_namespaces_and_deduplicate_substates() {
    let child = state_ref(Some("production"));
    let root = state_ref(None);
    assert_eq!(
      targets(&[child.clone(), child, root]).unwrap(),
      vec![
        ("cargo", "production.app".into()),
        ("vm", "production.app".into()),
        ("job", "migrate".into()),
        ("cargo", "global.app".into()),
        ("vm", "global.app".into()),
      ]
    );
  }

  #[test]
  fn stats_targets_reject_invalid_resource_names() {
    let mut state = state_ref(None);
    state.data.cargoes.as_mut().unwrap()[0].name = "invalid.name".into();
    assert!(targets(&[state]).is_err());
  }

  fn samples(name: &str) -> BTreeMap<String, ProcessStats> {
    BTreeMap::from([(
      name.to_owned(),
      ProcessStats {
        name: name.to_owned(),
        stats: serde_json::from_value(serde_json::json!({
          "cpu_stats": {"cpu_usage": {"total_usage": 200}, "system_cpu_usage": 1200, "online_cpus": 2},
          "precpu_stats": {"cpu_usage": {"total_usage": 100}, "system_cpu_usage": 1000},
          "memory_stats": {"usage": 805306368_u64, "limit": 1073741824_u64, "stats": {"cache": 268435456_u64}},
          "pids_stats": {"current": 4}
        }))
        .unwrap(),
      },
    )])
  }

  #[test]
  fn stats_compact_and_wide_preserve_usage_with_optional_details() {
    let stats = samples("production.web.r0.app.c");
    let compact = render_stats(&stats, false, 80);
    for expected in [
      "PROCESS", "CPU", "MEM", "MEM%", "100.00%", "512.0MiB", "50.00%",
    ] {
      assert!(compact.contains(expected), "missing {expected}");
    }
    for hidden in ["LIMIT", "1.00GiB", "NET I/O", "BLOCK I/O", "PIDS"] {
      assert!(!compact.contains(hidden), "unexpected {hidden}");
    }
    assert!(compact.lines().all(|line| measure_text_width(line) <= 80));
    let wide = render_stats(&stats, true, 80);
    for expected in [
      "MEM USAGE / LIMIT",
      "1.00GiB",
      "NET I/O",
      "BLOCK I/O",
      "PIDS",
    ] {
      assert!(wide.contains(expected), "missing {expected}");
    }
    assert!(wide.contains("production.web.r0.app.c"));
  }

  #[test]
  fn stats_narrow_output_preserves_full_ascii_and_unicode_process_names() {
    for name in [
      format!("production.{}.r0.app.c", "long-name-".repeat(12)),
      format!("production.{}.r0.app.c", "服务".repeat(40)),
    ] {
      for width in [20, 40, 80, 100] {
        let output = render_stats(&samples(&name), false, width);
        assert!(
          output.lines().all(|line| measure_text_width(line) <= width),
          "output exceeded {width} columns: {output}",
        );
        let identity = output
          .lines()
          .take_while(|line| !line.starts_with("  "))
          .collect::<String>();
        assert_eq!(identity, name);
        assert!(output.contains("512.0MiB"));
        assert!(output.contains("100.00%"));
        assert!(output.contains("50.00%"));
      }
    }
  }
}
