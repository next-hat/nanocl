//! Read-only, Statefile-scoped runtime monitoring.

use std::{
  collections::BTreeSet,
  io::{self, IsTerminal, Write},
  time::Duration,
};

use chrono::{NaiveDateTime, Utc};
use futures::{StreamExt, stream};
use nanocl_error::{
  http_client::{HttpClientError, HttpClientResult},
  io::{IoError, IoResult},
};
use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericClause, GenericFilter, GenericListQuery, GenericWhere},
    job::JobInspect,
    process::Process,
    statefile::Statefile,
    system::{Event, EventActorKind, ObjPsStatus, ObjPsStatusKind},
  },
};
use serde_json::json;
use tabled::{
  Table,
  settings::{
    Alignment, Modify, Padding, Style, Width,
    object::{Columns, Segment},
  },
};

use crate::{
  config::CliConfig,
  models::{StateRef, StateStatusOpts, StateStatusRow},
  utils::process::resource_key,
};

use super::{
  ArgParseMode, parse_build_args, parse_state_file_recurr, read_state_file,
};

mod process;

const REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(10);

fn input_error(stage: &str, err: IoError) -> IoError {
  IoError::with_context(
    format!("State status: {stage}"),
    io::Error::new(
      err.inner.kind(),
      "failed; details hidden to protect secret values",
    ),
  )
}

fn read_error(err: &HttpClientError) -> String {
  match err {
    HttpClientError::HttpError(err) => {
      format!("daemon read failed (HTTP {})", err.status.as_u16())
    }
    HttpClientError::IoError(_) => "daemon read failed".into(),
  }
}

/// Only an explicit 404 means the declared object is absent.
fn optional_read<T>(result: HttpClientResult<T>) -> Result<Option<T>, String> {
  match result {
    Ok(value) => Ok(Some(value)),
    Err(HttpClientError::HttpError(err)) if err.status.as_u16() == 404 => {
      Ok(None)
    }
    Err(err) => Err(read_error(&err)),
  }
}

fn targets(
  states: &[StateRef<Statefile>],
) -> IoResult<Vec<(EventActorKind, String)>> {
  let mut seen = BTreeSet::new();
  let mut targets = Vec::new();
  for state in states {
    let namespace = state.data.namespace.as_deref().unwrap_or("global");
    let mut insert = |kind: EventActorKind, key: String| {
      if seen.insert((kind.to_string(), key.clone())) {
        targets.push((kind, key));
      }
    };
    for cargo in state.data.cargoes.iter().flatten() {
      insert(EventActorKind::Cargo, resource_key(&cargo.name, namespace)?);
    }
    for vm in state.data.virtual_machines.iter().flatten() {
      insert(EventActorKind::Vm, resource_key(&vm.name, namespace)?);
    }
    for job in state.data.jobs.iter().flatten() {
      insert(EventActorKind::Job, job.name.clone());
    }
    for resource in state.data.resources.iter().flatten() {
      insert(EventActorKind::Resource, resource.name.clone());
    }
  }
  Ok(targets)
}

fn empty_row(kind: &EventActorKind, key: &str) -> StateStatusRow {
  StateStatusRow {
    kind: kind.to_string().to_lowercase(),
    name: display_text(key),
    running: "-".into(),
    status: "missing".into(),
    health: "unknown".into(),
    failure: "-".into(),
    read_failed: false,
  }
}

fn set_runtime(
  row: &mut StateStatusRow,
  status: &ObjPsStatus,
  instances: &[Process],
) {
  let running = instances
    .iter()
    .filter(|instance| {
      instance
        .data
        .state
        .as_ref()
        .is_some_and(|state| state.running == Some(true))
    })
    .count();
  row.running = format!("{running}/{}", instances.len());
  row.status = format!("{}/{}", status.actual, status.wanted);
  row.health = status.health.to_string();
}

/// Select just one recent failure, after scoping on the server. Each OR arm
/// repeats the time and actor predicates so unrelated events cannot win limit=1.
fn failure_filter(
  kind: &EventActorKind,
  key: &str,
  since: NaiveDateTime,
) -> GenericFilter {
  let mut alternatives = Vec::new();
  for relation in ["actor", "related"] {
    for (field, clause) in [
      ("kind", GenericClause::Eq("error".into())),
      (
        "action",
        GenericClause::In(vec!["fail".into(), "unhealthy".into()]),
      ),
    ] {
      alternatives.push(std::collections::HashMap::from([
        (
          relation.into(),
          GenericClause::Contains(json!({"Key": key, "Kind": kind})),
        ),
        (field.into(), clause),
        (
          "created_at".into(),
          GenericClause::Ge(since.and_utc().to_rfc3339()),
        ),
      ]));
    }
  }
  GenericFilter {
    r#where: Some(GenericWhere {
      conditions: alternatives.remove(0),
      or: Some(alternatives),
    }),
    limit: Some(1),
    order_by: Some(vec!["created_at desc".into()]),
    ..Default::default()
  }
}

async fn recent_event(
  client: &NanocldClient,
  kind: &EventActorKind,
  key: &str,
  since: NaiveDateTime,
) -> Result<Option<Event>, String> {
  let query = GenericListQuery::try_from(failure_filter(kind, key, since))
    .map_err(|_| "cannot build event filter".to_owned())?;
  let res = client
    .send_get("/events", Some(query))
    .await
    .map_err(|err| read_error(&err))?;
  let events = NanocldClient::res_json::<Vec<Event>>(res)
    .await
    .map_err(|err| read_error(&err))?;
  Ok(events.into_iter().next())
}

fn failure_detail(
  event: Option<Event>,
  observed: Option<(NaiveDateTime, String)>,
) -> Option<(NaiveDateTime, String)> {
  let Some(event) = event else {
    return observed;
  };
  // Completion notifications often say only "Job <name>". Keep the exit/OOM
  // or health-check detail, with its actual observation time, in that case.
  if event.reason == "state_sync"
    && matches!(event.action.as_str(), "fail" | "unhealthy")
    && observed.is_some()
  {
    return observed;
  }
  let reason = match event.note.filter(|note| !note.trim().is_empty()) {
    Some(note) => format!("{}: {}: {note}", event.action, event.reason),
    None => format!("{}: {}", event.action, event.reason),
  };
  Some((event.created_at, reason))
    .into_iter()
    .chain(observed)
    .max_by_key(|(time, _)| *time)
}

fn set_job(row: &mut StateStatusRow, job: &JobInspect) {
  set_runtime(row, &job.status, &job.instances);
  if job.spec.schedule.is_some()
    && job.status.actual == ObjPsStatusKind::Create
    && job.instances.is_empty()
  {
    row.status = "scheduled".into();
  } else if job.instance_success > 0 || job.instance_failed > 0 {
    row.status.push_str(&format!(
      " ({} succeeded, {} failed)",
      job.instance_success, job.instance_failed
    ));
  }
}

async fn inspect_target(
  client: &NanocldClient,
  kind: &EventActorKind,
  key: &str,
  row: &mut StateStatusRow,
  since: NaiveDateTime,
) -> Result<(NaiveDateTime, Option<(NaiveDateTime, String)>), String> {
  let (created_at, instances) = match kind {
    EventActorKind::Cargo => {
      match optional_read(client.inspect_cargo(key).await)? {
        Some(cargo) => {
          set_runtime(row, &cargo.status, &cargo.instances);
          (cargo.created_at, cargo.instances)
        }
        None => return Ok((since, None)),
      }
    }
    EventActorKind::Vm => match optional_read(client.inspect_vm(key).await)? {
      Some(vm) => {
        set_runtime(row, &vm.status, &vm.instances);
        (vm.created_at, vm.instances)
      }
      None => return Ok((since, None)),
    },
    EventActorKind::Job => {
      match optional_read(client.inspect_job(key).await)? {
        Some(job) => {
          set_job(row, &job);
          (job.created_at, job.instances)
        }
        None => return Ok((since, None)),
      }
    }
    EventActorKind::Resource => {
      match optional_read(client.inspect_resource(key).await)? {
        Some(resource) => {
          row.status = "present".into();
          row.health = "not reported".into();
          return Ok((since.max(resource.created_at), None));
        }
        None => return Ok((since, None)),
      }
    }
    _ => unreachable!("only Statefile runtime objects are collected"),
  };
  let since = since.max(created_at);
  Ok((since, process::recent_failure(&instances, since)))
}

async fn collect_row(
  client: &NanocldClient,
  kind: &EventActorKind,
  key: &str,
  since: NaiveDateTime,
) -> StateStatusRow {
  let mut row = empty_row(kind, key);
  let (since, observed) = match ntex::time::timeout(
    READ_TIMEOUT,
    inspect_target(client, kind, key, &mut row, since),
  )
  .await
  {
    Ok(Ok(value)) => value,
    result => {
      row.status = "unavailable".into();
      row.read_failed = true;
      row.failure = match result {
        Ok(Err(error)) => error,
        Err(_) => "daemon read timed out".into(),
        _ => unreachable!(),
      };
      return row;
    }
  };
  let event =
    ntex::time::timeout(READ_TIMEOUT, recent_event(client, kind, key, since))
      .await;
  let failure = match event {
    Ok(Ok(event)) => failure_detail(event, observed),
    result => {
      row.read_failed = true;
      let error = match result {
        Ok(Err(error)) => error,
        Err(_) => "daemon read timed out".into(),
        _ => unreachable!(),
      };
      row.failure = format!("event history unavailable: {error}");
      observed
    }
  };
  if let Some((time, reason)) = failure {
    let reason = format!(
      "{} UTC {}",
      time.format("%Y-%m-%d %H:%M:%S"),
      display_text(&reason)
    );
    row.failure = if row.read_failed {
      format!("{reason}; {}", row.failure)
    } else {
      reason
    };
  }
  row
}

/// Keep each field bounded and prevent control characters from changing the
/// terminal during watch refreshes.
fn display_text(value: &str) -> String {
  let normalized = value
    .chars()
    .map(|c| if c.is_control() { ' ' } else { c })
    .collect::<String>();
  let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
  let mut chars = normalized.chars();
  let mut output: String = chars.by_ref().take(200).collect();
  if chars.next().is_some() {
    output.push_str("...");
  }
  output
}

fn render(rows: &[StateStatusRow], now: NaiveDateTime, watch: bool) -> String {
  let mut output = format!(
    "Application status — {} UTC",
    now.format("%Y-%m-%d %H:%M:%S")
  );
  if watch {
    output.push_str(" — refreshing every 2s; Ctrl-C to stop");
  }
  output.push('\n');
  if rows.is_empty() {
    output.push_str("No cargoes, VMs, jobs, or resources declared.\n");
    return output;
  }
  let table = Table::new(rows)
    .with(Style::empty())
    .with(
      Modify::new(Segment::all())
        .with(Padding::new(0, 2, 0, 0))
        .with(Alignment::left()),
    )
    .with(
      Modify::new(Columns::new(5..6)).with(Width::wrap(64).keep_words(true)),
    )
    .to_string();
  output.push_str(&table);
  output.push_str("\nRunning: running/observed processes. Status: actual/wanted. Health: last reported.\nRecent failure: latest event or observed process failure in the last 24h.\n");
  output
}

pub(super) async fn exec_state_status(
  cli_conf: &CliConfig,
  opts: &StateStatusOpts,
) -> IoResult<()> {
  let state =
    read_state_file(&opts.source, &cli_conf.user_config.display_format)
      .await
      .map_err(|err| input_error("read Statefile", err))?;
  let args =
    parse_build_args(&state.data, ArgParseMode::Status, &opts.args, true)
      .map_err(|err| input_error("Statefile arguments", err))?;
  let states = parse_state_file_recurr(cli_conf, &state, &args, true)
    .await
    .map_err(|err| input_error("render Statefile", err))?;
  let targets =
    targets(&states).map_err(|err| input_error("declarations", err))?;
  let terminal = io::stdout().is_terminal()
    && std::env::var("TERM").as_deref() != Ok("dumb");
  loop {
    let now = Utc::now().naive_utc();
    let since = now - chrono::Duration::hours(24);
    // Apply uses the configured daemon for runtime objects, even when a
    // Statefile selects another API endpoint for template lookups.
    let rows = stream::iter(
      targets
        .iter()
        .map(|(kind, key)| collect_row(&cli_conf.client, kind, key, since)),
    )
    .buffered(8)
    .collect::<Vec<_>>()
    .await;
    let output = render(&rows, now, opts.watch);
    {
      let mut stdout = io::stdout().lock();
      if opts.watch && terminal {
        stdout.write_all(b"\x1b[2J\x1b[H")?;
      }
      stdout.write_all(output.as_bytes())?;
      stdout.flush()?;
    }
    if !opts.watch || targets.is_empty() {
      if rows.iter().any(|row| row.read_failed) {
        return Err(IoError::other(
          "State status",
          "some status reads failed; see unavailable entries",
        ));
      }
      return Ok(());
    }
    ntex::time::sleep(REFRESH_INTERVAL).await;
  }
}

#[cfg(test)]
mod tests;
