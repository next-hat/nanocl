//! Read-only, Statefile-scoped runtime monitoring.

use std::{
  collections::BTreeSet,
  io::{self, IsTerminal, Write},
  time::Duration,
};

use bollard_next::models::{ContainerState, HealthStatusEnum};
use chrono::{NaiveDateTime, Utc};
use dialoguer::console::measure_text_width;
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
    process::{Process, ProcessKind},
    statefile::Statefile,
    system::{
      Event, EventActorKind, EventKind, NativeEventAction, ObjPsStatus,
      ObjPsStatusKind,
    },
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
  utils::{
    print::{terminal_width, wrap_lines},
    process::resource_key,
  },
};

use super::{
  ArgParseMode, parse_build_args, parse_state_file_recurr, read_state_file,
};

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
    name: sanitize_identity(key),
    running: "-".into(),
    status: "missing".into(),
    health: "unknown".into(),
    failure: "-".into(),
    read_failed: false,
    state: "missing".into(),
    wanted: None,
    results: None,
    recent_failure: None,
    read_error: None,
  }
}

fn state_label(status: &ObjPsStatusKind) -> &'static str {
  match status {
    ObjPsStatusKind::Create => "created",
    ObjPsStatusKind::Starting => "starting",
    ObjPsStatusKind::Start => "running",
    ObjPsStatusKind::Updating => "updating",
    ObjPsStatusKind::Update => "updated",
    ObjPsStatusKind::Destroying => "removing",
    ObjPsStatusKind::Destroy => "removed",
    ObjPsStatusKind::Stopping => "stopping",
    ObjPsStatusKind::Stop => "stopped",
    ObjPsStatusKind::Fail => "failed",
    ObjPsStatusKind::Finish => "completed",
    ObjPsStatusKind::Unknown => "unknown",
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
  row.state = state_label(&status.actual).into();
  row.wanted = (status.actual != status.wanted)
    .then(|| state_label(&status.wanted).to_owned());
  row.health = status.health.to_string();
}

/// Select just one recent failure, after scoping on the server. Each OR arm
/// repeats the time and actor predicates so unrelated events cannot win limit=1.
fn failure_filter(
  kind: &EventActorKind,
  key: &str,
  since: NaiveDateTime,
  job: Option<&JobInspect>,
) -> GenericFilter {
  let mut alternatives = Vec::new();
  for (relation, run_key) in
    [("actor", "actor_run_key"), ("related", "related_run_key")]
  {
    for (field, clause) in [
      ("kind", GenericClause::Eq("error".into())),
      (
        "action",
        GenericClause::In(vec!["fail".into(), "unhealthy".into()]),
      ),
    ] {
      let mut branch = std::collections::HashMap::from([
        (
          relation.into(),
          GenericClause::Contains(json!({"Key": key, "Kind": kind})),
        ),
        (field.into(), clause),
        (
          "created_at".into(),
          GenericClause::Ge(since.and_utc().to_rfc3339()),
        ),
      ]);
      if let Some(job) = job {
        if let Some(run) = job.current_run_key {
          let mut current = branch.clone();
          current.insert(run_key.into(), GenericClause::Eq(run.to_string()));
          alternatives.push(current);
        }
        // Job-level errors can be emitted without an execution identity.
        branch.insert(run_key.into(), GenericClause::IsNull);
      }
      alternatives.push(branch);
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
  filter: GenericFilter,
) -> Result<Option<Event>, String> {
  let query = GenericListQuery::try_from(filter)
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
  let note = event
    .note
    .as_deref()
    .map(str::trim)
    .filter(|note| !note.is_empty());
  // An actor label or a status-only note adds no cause to a process failure.
  if observed.is_some()
    && event.reason == "state_sync"
    && let Ok(action @ (NativeEventAction::Fail | NativeEventAction::Unhealthy)) =
      event.action.parse()
    && note.is_none_or(|note| {
      event.actor.as_ref().is_some_and(|actor| {
        note == action.lifecycle_note(actor)
          || note.split_once(' ').is_some_and(|(kind, key)| {
            actor.key.as_deref() == Some(key)
              && kind.eq_ignore_ascii_case(&actor.kind.to_string())
          })
      })
    })
  {
    return observed;
  }
  let reason = match note {
    Some(note) if event.reason == "state_sync" => note.to_owned(),
    Some(note) => format!("{}: {}: {note}", event.action, event.reason),
    None => format!("{}: {}", event.action, event.reason),
  };
  Some((event.created_at, reason))
    .into_iter()
    .chain(observed)
    .max_by_key(|(time, _)| *time)
}

// Docker reuses a VM container ID across restarts. Only the run observed by
// the die event can contribute a cause to that event.
fn same_vm_exit(metadata: &serde_json::Value, state: &ContainerState) -> bool {
  let nanos = |value: &str| {
    chrono::DateTime::parse_from_rfc3339(value)
      .ok()
      .and_then(|t| t.timestamp_nanos_opt())
  };
  let (Some(event), Some(start)) = (
    metadata["DockerEventTimeNano"].as_i64(),
    metadata["StartedAt"].as_str().and_then(nanos),
  ) else {
    return false;
  };
  if start > event || state.started_at.as_deref().and_then(nanos) != Some(start)
  {
    return false;
  }
  let finished = state.finished_at.as_deref().and_then(nanos);
  let event_finished = metadata["FinishedAt"].as_str().and_then(nanos);
  finished.is_none_or(|end| (start..=event).contains(&end))
    && event_finished.is_none_or(|end| {
      (start..=event).contains(&end) && finished.is_none_or(|seen| seen == end)
    })
}

/// Locate the inventory snapshot for the execution reported by the event.
fn matching_exit(event: &Event, processes: &[Process]) -> Option<usize> {
  let actor = event.actor.as_ref()?;
  if event.reporting_controller != "nanocl.io/core"
    || event.reason != "state_sync"
    || event.kind != EventKind::Error
  {
    return None;
  }
  let metadata = event.metadata.as_ref()?;
  let key = actor.key.as_deref()?;
  let id = metadata["ContainerId"].as_str()?;
  let code = metadata["ExitCode"].as_i64()?;
  processes.iter().position(|process| {
    let Some(state) = process.data.state.as_ref() else {
      return false;
    };
    if process.key != id
      || process.kind_key != key
      || process.node_name != event.reporting_node
      || state.running != Some(false)
      || state.restarting == Some(true)
      || state.exit_code != Some(code)
    {
      return false;
    }
    match (&actor.kind, &process.kind, event.action.as_str()) {
      (EventActorKind::Vm, ProcessKind::Vm, "die") => {
        same_vm_exit(metadata, state)
      }
      (EventActorKind::Job, ProcessKind::Job, "fail") => {
        let run = actor
          .attributes
          .as_ref()
          .and_then(|attr| attr["RunKey"].as_str());
        let process_run = process
          .data
          .config
          .as_ref()
          .and_then(|config| config.labels.as_ref())
          .and_then(|labels| labels.get("io.nanocl.job.run"));
        process_run.is_some_and(|label| Some(label.as_str()) == run)
      }
      _ => false,
    }
  })
}

fn status_failure(
  mut event: Option<Event>,
  mut processes: Vec<Process>,
  since: NaiveDateTime,
) -> Option<(NaiveDateTime, String)> {
  if let Some(event) = &mut event
    && let Some(index) = matching_exit(event, &processes)
  {
    // Inventory refresh time is not a new failure of this exited container.
    if let Some(note) =
      event.note.as_mut().filter(|note| !note.trim().is_empty())
    {
      let process = processes.remove(index);
      if process
        .data
        .state
        .is_some_and(|state| state.oom_killed == Some(true))
        && !event
          .metadata
          .as_ref()
          .is_some_and(|metadata| metadata["OOMKilled"] == true)
      {
        note.push_str(" (OOM killed)");
      }
    } else {
      // Use the observed reason at the event time when the event has no note.
      processes[index].updated_at = event.created_at;
    }
  }
  failure_detail(event, recent_failure(&processes, since))
}

/// Latest observed process failure without exposing health-check log output.
fn recent_failure(
  processes: &[Process],
  since: chrono::NaiveDateTime,
) -> Option<(chrono::NaiveDateTime, String)> {
  processes
    .iter()
    .filter(|process| process.updated_at >= since)
    .filter_map(|process| {
      let state = process.data.state.as_ref()?;
      let reason = if state.oom_killed == Some(true) {
        "out of memory (OOM killed)".to_owned()
      } else if let Some(error) = state
        .error
        .as_deref()
        .filter(|error| !error.trim().is_empty())
      {
        error.trim().to_owned()
      } else if state.running != Some(true)
        && let Some(code) = state.exit_code.filter(|code| *code != 0)
      {
        format!("exited with code {code}")
      } else if let Some(health) = state
        .health
        .as_ref()
        .filter(|health| health.status == Some(HealthStatusEnum::UNHEALTHY))
      {
        let code = health.log.as_ref().and_then(|entries| {
          entries
            .iter()
            .rev()
            .find_map(|entry| entry.exit_code.filter(|code| *code != 0))
        });
        match code {
          Some(code) => {
            format!("unhealthy (health check exited with code {code})")
          }
          None => "unhealthy".to_owned(),
        }
      } else {
        return None;
      };
      Some((process.updated_at, format!("{}: {reason}", process.name)))
    })
    .max_by_key(|(updated_at, _)| *updated_at)
}

fn set_job(row: &mut StateStatusRow, job: &JobInspect) {
  set_runtime(row, &job.status, &job.instances);
  row.results = None;
  // A run ending successfully or with failure is a job outcome, not a pending
  // transition back to its creation/start request.
  if matches!(
    job.status.actual,
    ObjPsStatusKind::Finish | ObjPsStatusKind::Fail
  ) && matches!(
    job.status.wanted,
    ObjPsStatusKind::Create | ObjPsStatusKind::Start
  ) {
    row.wanted = None;
  }
  if job.spec.schedule.is_some()
    && job.status.actual == ObjPsStatusKind::Create
    && job.instances.is_empty()
  {
    row.status = "scheduled".into();
    row.state = "scheduled".into();
    row.wanted = None;
  } else if job.instance_success > 0 || job.instance_failed > 0 {
    row.status.push_str(&format!(
      " ({} succeeded, {} failed)",
      job.instance_success, job.instance_failed
    ));
    row.results = Some(format!(
      "{} succeeded, {} failed",
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
) -> Result<(NaiveDateTime, GenericFilter, Vec<Process>), String> {
  let history = |created_at, job: Option<&JobInspect>| {
    let since = since.max(created_at);
    (since, failure_filter(kind, key, since, job))
  };
  let target = match kind {
    EventActorKind::Cargo => optional_read(client.inspect_cargo(key).await)?
      .map(|cargo| {
        set_runtime(row, &cargo.status, &cargo.instances);
        (history(cargo.created_at, None), cargo.instances)
      }),
    EventActorKind::Vm => {
      optional_read(client.inspect_vm(key).await)?.map(|vm| {
        set_runtime(row, &vm.status, &vm.instances);
        (history(vm.created_at, None), vm.instances)
      })
    }
    EventActorKind::Job => {
      optional_read(client.inspect_job(key).await)?.map(|job| {
        set_job(row, &job);
        (history(job.created_at, Some(&job)), job.instances)
      })
    }
    EventActorKind::Resource => {
      optional_read(client.inspect_resource(key).await)?.map(|resource| {
        row.status = "present".into();
        row.state = "present".into();
        row.health = "not reported".into();
        (history(resource.created_at, None), Vec::new())
      })
    }
    _ => unreachable!("only Statefile runtime objects are collected"),
  };
  let ((since, filter), instances) =
    target.unwrap_or_else(|| (history(since, None), Vec::new()));
  Ok((since, filter, instances))
}

async fn collect_row(
  client: &NanocldClient,
  kind: &EventActorKind,
  key: &str,
  since: NaiveDateTime,
) -> StateStatusRow {
  let mut row = empty_row(kind, key);
  let (since, filter, instances) = match ntex::time::timeout(
    READ_TIMEOUT,
    inspect_target(client, kind, key, &mut row, since),
  )
  .await
  {
    Ok(Ok(value)) => value,
    result => {
      row.status = "unavailable".into();
      row.state = "unavailable".into();
      row.read_failed = true;
      row.failure = match result {
        Ok(Err(error)) => error,
        Err(_) => "daemon read timed out".into(),
        _ => unreachable!(),
      };
      row.read_error = Some(row.failure.clone());
      return row;
    }
  };
  let event =
    ntex::time::timeout(READ_TIMEOUT, recent_event(client, filter)).await;
  let event = match event {
    Ok(Ok(event)) => event,
    result => {
      row.read_failed = true;
      let error = match result {
        Ok(Err(error)) => error,
        Err(_) => "daemon read timed out".into(),
        _ => unreachable!(),
      };
      row.failure = format!("event history unavailable: {error}");
      row.read_error = Some(row.failure.clone());
      None
    }
  };
  if let Some((time, reason)) = status_failure(event, instances, since) {
    let reason = display_text(&reason);
    row.recent_failure = Some((time, reason.clone()));
    let reason = format!("{} UTC {}", time.format("%Y-%m-%d %H:%M:%S"), reason);
    row.failure = if row.read_failed {
      format!("{reason}; {}", row.failure)
    } else {
      reason
    };
  }
  row
}

/// Remove terminal controls without changing ordinary identity whitespace.
fn sanitize_identity(value: &str) -> String {
  value
    .chars()
    .map(|c| if c.is_control() { ' ' } else { c })
    .collect()
}

/// Keep diagnostic text bounded during watch refreshes.
fn display_text(value: &str) -> String {
  let normalized = sanitize_identity(value)
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");
  let mut chars = normalized.chars();
  let mut output: String = chars.by_ref().take(200).collect();
  if chars.next().is_some() {
    output.push_str("...");
  }
  output
}

fn render_wide(
  rows: &[StateStatusRow],
  now: NaiveDateTime,
  watch: bool,
) -> String {
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

fn failure_age(time: NaiveDateTime, now: NaiveDateTime) -> String {
  let seconds = (now - time).num_seconds().max(0);
  if seconds < 60 {
    format!("{seconds}s")
  } else if seconds < 3600 {
    format!("{}m", seconds / 60)
  } else {
    format!("{}h", seconds / 3600)
  }
}

fn append_wrapped(
  output: &mut String,
  text: &str,
  width: usize,
  indent: usize,
) {
  output.push_str(&wrap_lines(text, width, indent));
  output.push('\n');
}

fn append_details(
  output: &mut String,
  row: &StateStatusRow,
  now: NaiveDateTime,
  width: usize,
) {
  if let Some(wanted) = &row.wanted {
    append_wrapped(output, &format!("Wanted: {wanted}"), width, 2);
  }
  if let Some(results) = &row.results {
    append_wrapped(output, &format!("Results: {results}"), width, 2);
  }
  if let Some((time, reason)) = &row.recent_failure {
    append_wrapped(
      output,
      &format!("Last failure {} ago: {reason}", failure_age(*time, now)),
      width,
      2,
    );
  }
  if let Some(error) = &row.read_error {
    append_wrapped(output, error, width, 2);
  }
}

fn padded(value: &str, width: usize) -> String {
  format!(
    "{value}{}",
    " ".repeat(width.saturating_sub(measure_text_width(value)))
  )
}

fn render(
  rows: &[StateStatusRow],
  now: NaiveDateTime,
  watch: bool,
  wide: bool,
  width: usize,
) -> String {
  if wide {
    return render_wide(rows, now, watch);
  }
  let width = width.max(1);
  let mut output = String::new();
  append_wrapped(
    &mut output,
    &format!("State status  {} UTC", now.format("%Y-%m-%d %H:%M:%S")),
    width,
    0,
  );
  if watch {
    append_wrapped(&mut output, "Refresh 2s; Ctrl-C to stop", width, 0);
  }
  if rows.is_empty() {
    append_wrapped(
      &mut output,
      "No cargoes, VMs, jobs, or resources declared.",
      width,
      0,
    );
    return output;
  }

  let objects = rows
    .iter()
    .map(|row| format!("{}/{}", row.kind, row.name))
    .collect::<Vec<_>>();
  let state_width = rows
    .iter()
    .map(|row| measure_text_width(&row.state))
    .max()
    .unwrap_or(0)
    .max(5);
  let proc_width = rows
    .iter()
    .map(|row| measure_text_width(&row.running))
    .max()
    .unwrap_or(0)
    .max(4);
  let health_width = rows
    .iter()
    .map(|row| measure_text_width(&row.health))
    .max()
    .unwrap_or(0)
    .max(6);
  let available_object_width =
    width.saturating_sub(state_width + proc_width + health_width + 6);
  let object_width = available_object_width.min(
    objects
      .iter()
      .map(|object| measure_text_width(object))
      .max()
      .unwrap_or(0)
      .max(12),
  );
  // Keep the state columns aligned whenever there is useful room for an
  // identity. On narrow terminals, stack labelled fields under the full name.
  if available_object_width >= 20 {
    output.push_str(&format!(
      "{}  {}  {}  HEALTH\n",
      padded("OBJECT", object_width),
      padded("STATE", state_width),
      padded("PROC", proc_width),
    ));
    for (row, object) in rows.iter().zip(&objects) {
      let wrapped = wrap_lines(object, object_width, 0);
      for (index, line) in wrapped.lines().enumerate() {
        if index == 0 {
          output.push_str(&format!(
            "{}  {}  {}  {}\n",
            padded(line, object_width),
            padded(&row.state, state_width),
            padded(&row.running, proc_width),
            row.health,
          ));
        } else {
          output.push_str(line);
          output.push('\n');
        }
      }
      append_details(&mut output, row, now, width);
    }
  } else {
    for (row, object) in rows.iter().zip(&objects) {
      append_wrapped(&mut output, object, width, 0);
      append_wrapped(
        &mut output,
        &format!(
          "State: {}  Proc: {}  Health: {}",
          row.state, row.running, row.health,
        ),
        width,
        2,
      );
      append_details(&mut output, row, now, width);
    }
  }
  append_wrapped(
    &mut output,
    "Proc: running/observed processes, including init, sandbox and sidecars.",
    width,
    0,
  );
  append_wrapped(
    &mut output,
    "Health: last reported. Last failure: latest in 24h.",
    width,
    0,
  );
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
    let output = render(&rows, now, opts.watch, opts.wide, terminal_width());
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
mod tests {
  use bollard_next::models::{
    ContainerInspectResponse, ContainerState, Health, HealthcheckResult,
  };
  use nanocl_error::http::HttpError;
  use nanocld_client::stubs::{
    cargo_spec::CargoSpec,
    job::{JobPartial, JobSpec},
    resource::ResourcePartial,
    system::ObjPsHealthStatusKind,
    vm_spec::VmSpecPartial,
  };

  use super::*;

  fn time(seconds: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(seconds, 0)
      .unwrap()
      .naive_utc()
  }

  fn state_ref(namespace: Option<&str>, location: &str) -> StateRef<Statefile> {
    let mut data: Statefile =
      serde_json::from_value(json!({"ApiVersion": "v0.18"})).unwrap();
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
      data: json!({}),
      metadata: None,
    }]);
    StateRef {
      raw: String::new(),
      format: Default::default(),
      data,
      root: Default::default(),
      location: location.into(),
    }
  }

  fn instance(running: Option<bool>) -> Process {
    Process {
      key: "instance".into(),
      created_at: time(10),
      updated_at: time(10),
      name: "app".into(),
      kind: ProcessKind::Job,
      node_name: "node".into(),
      kind_key: "app".into(),
      ip_address: None,
      data: ContainerInspectResponse {
        state: Some(ContainerState {
          running,
          finished_at: Some("0001-01-01T00:00:00Z".into()),
          ..Default::default()
        }),
        ..Default::default()
      },
    }
  }

  fn event(seconds: i64, action: &str, reason: &str, note: &str) -> Event {
    serde_json::from_value(json!({
      "Key": "00000000-0000-0000-0000-000000000001",
      "CreatedAt": time(seconds), "ExpiresAt": time(seconds + 86400),
      "ReportingNode": "node", "ReportingController": "nanocl.io/core",
      "Kind": "error", "Action": action, "Reason": reason, "Note": note
    }))
    .unwrap()
  }

  fn process(name: &str, seconds: i64, state: ContainerState) -> Process {
    let updated_at = chrono::DateTime::from_timestamp(seconds, 0)
      .unwrap()
      .naive_utc();
    Process {
      key: name.to_owned(),
      created_at: updated_at,
      updated_at,
      name: name.to_owned(),
      kind: ProcessKind::Cargo,
      node_name: "node-a".to_owned(),
      kind_key: "global.app".to_owned(),
      ip_address: None,
      data: ContainerInspectResponse {
        state: Some(state),
        ..Default::default()
      },
    }
  }

  #[test]
  fn status_targets_include_all_kinds_and_deduplicate_substates() {
    let child = state_ref(Some("production"), "child.yml");
    let duplicate = child.clone();
    let root = state_ref(None, "Statefile.yml");
    let found = targets(&[child, duplicate, root]).unwrap();
    assert_eq!(
      found,
      vec![
        (EventActorKind::Cargo, "production.app".into()),
        (EventActorKind::Vm, "production.app".into()),
        (EventActorKind::Job, "migrate".into()),
        (EventActorKind::Resource, "route".into()),
        (EventActorKind::Cargo, "global.app".into()),
        (EventActorKind::Vm, "global.app".into()),
      ]
    );
  }

  #[test]
  fn status_optional_read_only_treats_http_404_as_missing() {
    assert_eq!(optional_read(Ok(7)), Ok(Some(7)));
    assert_eq!(
      optional_read::<()>(Err(HttpError::not_found("private body").into())),
      Ok(None)
    );
    for status in [401, 403, 500] {
      let error = HttpError::new(
        ntex::http::StatusCode::from_u16(status).unwrap(),
        "private body",
      );
      assert_eq!(
        optional_read::<()>(Err(error.into())),
        Err(format!("daemon read failed (HTTP {status})"))
      );
    }
    let error = IoError::with_context(
      "private path",
      io::Error::new(io::ErrorKind::NotFound, "private body"),
    );
    assert_eq!(
      optional_read::<()>(Err(error.into())),
      Err("daemon read failed".into())
    );
  }

  #[test]
  fn status_failure_filter_scopes_every_branch_and_orders_by_recency() {
    let since = time(123);
    let filter = failure_filter(&EventActorKind::Job, "migrate", since, None);
    assert_eq!(filter.limit, Some(1));
    assert_eq!(filter.order_by, Some(vec!["created_at desc".into()]));
    let predicates = filter.r#where.unwrap();
    assert!(!predicates.conditions.is_empty());
    let branches: Vec<_> = std::iter::once(predicates.conditions)
      .chain(predicates.or.unwrap())
      .collect();
    assert_eq!(branches.len(), 4);
    let mut scopes = BTreeSet::new();
    for branch in branches {
      assert_eq!(branch.len(), 3);
      assert!(
        matches!(branch.get("created_at"), Some(GenericClause::Ge(value)) if value == &since.and_utc().to_rfc3339())
      );
      let relation = if branch.contains_key("actor") {
        "actor"
      } else {
        "related"
      };
      assert!(
        matches!(branch.get(relation), Some(GenericClause::Contains(value)) if value == &json!({"Key": "migrate", "Kind": "Job"}))
      );
      let failure = if let Some(GenericClause::Eq(kind)) = branch.get("kind") {
        assert_eq!(kind, "error");
        "error"
      } else {
        assert!(
          matches!(branch.get("action"), Some(GenericClause::In(actions)) if actions == &vec!["fail".to_owned(), "unhealthy".to_owned()])
        );
        "action"
      };
      scopes.insert((relation, failure));
    }
    assert_eq!(scopes.len(), 4);
  }

  #[test]
  fn status_runtime_counts_only_running_and_preserves_stopped_health() {
    let status = ObjPsStatus {
      actual: ObjPsStatusKind::Stop,
      wanted: ObjPsStatusKind::Stop,
      health: ObjPsHealthStatusKind::Unhealthy,
      ..Default::default()
    };
    let mut row = empty_row(&EventActorKind::Vm, "global.app");
    set_runtime(
      &mut row,
      &status,
      &[instance(Some(true)), instance(Some(false)), instance(None)],
    );
    assert_eq!(row.running, "1/3");
    assert_eq!(row.status, "stop/stop");
    assert_eq!(row.state, "stopped");
    assert_eq!(row.wanted, None);
    assert_eq!(row.health, "unhealthy");
  }

  fn scheduled_job() -> JobInspect {
    let spec_key = uuid::Uuid::new_v4();
    JobInspect {
      incarnation_key: uuid::Uuid::new_v4(),
      spec_key,
      current_run_key: None,
      created_at: time(10),
      updated_at: time(10),
      status: Default::default(),
      instance_total: 0,
      instance_success: 0,
      instance_running: 0,
      instance_failed: 0,
      spec: JobSpec {
        key: spec_key,
        job_key: "migrate".into(),
        version: "v0.19".into(),
        created_at: time(10),
        name: "migrate".into(),
        schedule: Some("@daily".into()),
        ..Default::default()
      },
      instances: vec![],
    }
  }

  #[test]
  fn status_job_failure_filter_excludes_previous_runs() {
    let mut job = scheduled_job();
    let failed_run = uuid::Uuid::from_u128(1);
    let successful_run = uuid::Uuid::from_u128(2);
    for (run, status) in [
      (None, ObjPsStatusKind::Create),
      (Some(failed_run), ObjPsStatusKind::Fail),
      (Some(successful_run), ObjPsStatusKind::Finish),
    ] {
      job.current_run_key = run;
      job.status.actual = status;
      let filter =
        failure_filter(&EventActorKind::Job, "migrate", time(10), Some(&job));
      assert_eq!(filter.limit, Some(1));
      let predicates = filter.r#where.unwrap();
      let branches: Vec<_> = std::iter::once(predicates.conditions)
        .chain(predicates.or.unwrap())
        .collect();
      let run_key = run.map(|key| key.to_string());
      let expected = BTreeSet::from([None, run_key.as_deref()]);
      for relation in ["actor", "related"] {
        let column = format!("{relation}_run_key");
        for failure in ["kind", "action"] {
          let eligible = branches
            .iter()
            .filter(|branch| {
              branch.contains_key(relation) && branch.contains_key(failure)
            })
            .map(|branch| match branch.get(&column) {
              Some(GenericClause::Eq(key)) => Some(key.as_str()),
              Some(GenericClause::IsNull) => None,
              other => {
                panic!("missing run constraint for {relation}: {other:?}")
              }
            })
            .collect::<BTreeSet<_>>();
          assert_eq!(eligible, expected, "{relation} {failure}");
        }
      }
      assert_eq!(branches.len(), 4 * expected.len());
    }
  }

  #[test]
  fn status_job_distinguishes_schedule_from_completed_runs() {
    let mut job = scheduled_job();
    let mut row = empty_row(&EventActorKind::Job, "migrate");
    set_job(&mut row, &job);
    assert_eq!(row.status, "scheduled");
    assert_eq!(row.state, "scheduled");
    assert_eq!(row.wanted, None);
    assert_eq!(row.running, "0/0");
    assert_eq!(row.health, "unknown");
    job.status.actual = ObjPsStatusKind::Finish;
    job.instance_success = 2;
    set_job(&mut row, &job);
    assert_eq!(row.status, "finish/create (2 succeeded, 0 failed)");
    assert_eq!(row.state, "completed");
    assert_eq!(row.results.as_deref(), Some("2 succeeded, 0 failed"));
    assert_eq!(row.wanted, None);
    job.status.actual = ObjPsStatusKind::Fail;
    job.instance_failed = 1;
    set_job(&mut row, &job);
    assert_eq!(row.status, "fail/create (2 succeeded, 1 failed)");
    assert_eq!(row.state, "failed");
    assert_eq!(row.results.as_deref(), Some("2 succeeded, 1 failed"));
    assert_eq!(row.wanted, None);
    job.spec.schedule = None;
    job.status.actual = ObjPsStatusKind::Create;
    job.instance_success = 0;
    job.instance_failed = 0;
    set_job(&mut row, &job);
    assert_eq!(row.status, "create/create");
    assert_eq!(row.results, None);
  }

  #[test]
  fn status_failure_detail_preserves_process_reason_for_generic_events() {
    let status_event = |action, note, kind, key| {
      let mut event = event(20, action, "state_sync", note);
      event.actor = Some(
        serde_json::from_value(json!({"Key": key, "Kind": kind})).unwrap(),
      );
      event
    };
    for reason in [
      "app: exited with code 127",
      "app: out of memory (OOM killed)",
    ] {
      let observed = Some((time(10), reason.into()));
      assert_eq!(
        failure_detail(
          Some(status_event("fail", "Job migrate failed", "Job", "migrate")),
          observed.clone()
        ),
        observed
      );
    }
    let observed = Some((
      time(10),
      "app: unhealthy (health check exited with code 2)".into(),
    ));
    for note in ["Cargo app is unhealthy", ""] {
      assert_eq!(
        failure_detail(
          Some(status_event("unhealthy", note, "Cargo", "app")),
          observed.clone()
        ),
        observed
      );
    }
  }

  #[test]
  fn actor_only_failure_notes_do_not_hide_the_process_cause() {
    let observed =
      Some((time(10), "worker: out of memory (OOM killed)".into()));
    for (action, kind, key, label) in [
      ("fail", "Job", "migrate", "Job migrate"),
      ("unhealthy", "Cargo", "app", "Cargo app"),
      ("fail", "Vm", "guest", "Vm guest"),
      ("fail", "Vm", "guest", "VM guest"),
    ] {
      let mut failure = event(20, action, "state_sync", label);
      failure.actor = Some(
        serde_json::from_value(json!({"Key": key, "Kind": kind})).unwrap(),
      );
      assert_eq!(
        failure_detail(Some(failure.clone()), observed.clone()),
        observed
      );
      assert_eq!(
        failure_detail(Some(failure.clone()), None),
        Some((time(20), label.into()))
      );
      let detail = format!("{label} exited with code 7");
      failure.note = Some(detail.clone());
      assert_eq!(
        failure_detail(Some(failure), observed.clone()),
        Some((time(20), detail))
      );
    }
  }

  fn assert_ordinary_exit_keeps_event_time(event: &Event, process: &Process) {
    let mut refreshed = process.clone();
    refreshed.updated_at = time(30);
    assert_eq!(
      status_failure(Some(event.clone()), vec![refreshed.clone()], time(0)),
      Some((event.created_at, event.note.clone().unwrap()))
    );
    let mut without_note = event.clone();
    without_note.note = None;
    assert_eq!(
      status_failure(Some(without_note), vec![refreshed.clone()], time(0)),
      Some((event.created_at, "app: exited with code 137".into()))
    );

    let mut newer = process.clone();
    newer.key = "another-container".into();
    newer.name = "worker".into();
    newer.updated_at = time(25);
    newer.data.state.as_mut().unwrap().exit_code = Some(7);
    assert_eq!(
      status_failure(Some(event.clone()), vec![refreshed, newer], time(0)),
      Some((time(25), "worker: exited with code 7".into()))
    );
  }

  #[test]
  fn status_adds_oom_only_for_the_exited_container_of_the_failed_run() {
    let mut terminal = event(
      20,
      "fail",
      "state_sync",
      "Job app failed: Step 1 exited with code 137",
    );
    terminal.actor = Some(
      serde_json::from_value(json!({
        "Kind": "Job", "Key":"app", "Attributes": {"RunKey": "run-app"}
      }))
      .unwrap(),
    );
    terminal.metadata =
      Some(json!({"ContainerId": "instance", "ExitCode": 137}));
    let mut failed = instance(Some(false));
    failed.data.config = Some(
      serde_json::from_value(json!({
        "Labels": {"io.nanocl.job.run": "run-app"}
      }))
      .unwrap(),
    );
    let state = failed.data.state.as_mut().unwrap();
    state.oom_killed = Some(true);
    state.exit_code = Some(137);
    let expected = terminal.note.clone().unwrap();
    let select = |event: Event, process: &Process| {
      status_failure(Some(event), vec![process.clone()], time(0)).unwrap()
    };
    assert_eq!(
      select(terminal.clone(), &failed),
      (time(20), format!("{expected} (OOM killed)"))
    );
    let mut other = terminal.clone();
    other.metadata.as_mut().unwrap()["ContainerId"] = "other-step".into();
    assert_eq!(select(other, &failed), (time(20), expected.clone()));
    let mut other = terminal.clone();
    other.actor.as_mut().unwrap().attributes.as_mut().unwrap()["RunKey"] =
      "next-run".into();
    assert_eq!(select(other, &failed), (time(20), expected.clone()));
    let mut other = terminal.clone();
    other.metadata = None;
    assert_eq!(select(other, &failed), (time(20), expected.clone()));
    failed.data.state.as_mut().unwrap().oom_killed = None;
    assert_eq!(select(terminal.clone(), &failed), (time(20), expected));
    assert_ordinary_exit_keeps_event_time(&terminal, &failed);

    terminal.metadata.as_mut().unwrap()["OOMKilled"] = true.into();
    let oom_note = "Job app failed: Step 1 ran out of memory (exit code 137)";
    terminal.note = Some(oom_note.into());
    failed.updated_at = time(30);
    assert_eq!(
      select(terminal.clone(), &failed),
      (time(20), oom_note.into())
    );
    let mut unrelated = failed.clone();
    unrelated.key = "another-step".into();
    unrelated.updated_at = time(25);
    unrelated.data.state.as_mut().unwrap().exit_code = Some(7);
    assert_eq!(
      status_failure(Some(terminal), vec![failed, unrelated], time(0)),
      Some((time(25), "app: exited with code 7".into()))
    );
  }

  #[test]
  fn status_vm_oom_matches_the_exited_run_of_a_restarted_container() {
    let note = "VM guest stopped unexpectedly when process app ran out of memory (exit code 137)";
    let mut die = event(20, "die", "state_sync", note);
    die.actor =
      Some(serde_json::from_value(json!({"Kind":"Vm","Key":"guest"})).unwrap());
    let start = "1970-01-01T00:00:05Z";
    let finish = "1970-01-01T00:00:15Z";
    die.metadata = Some(json!({
      "ContainerId":"instance", "ExitCode":137, "OOMKilled":true,
      "DockerEventTimeNano":16_000_000_000_i64, "StartedAt":start, "FinishedAt":finish,
    }));
    let mut snapshot = instance(Some(false));
    snapshot.kind = ProcessKind::Vm;
    snapshot.kind_key = "guest".into();
    snapshot.updated_at = time(30);
    let state = snapshot.data.state.as_mut().unwrap();
    state.started_at = Some(start.into());
    state.finished_at = Some(finish.into());
    state.exit_code = Some(137);
    let select = |event: &Event, process: &Process| {
      status_failure(Some(event.clone()), vec![process.clone()], time(0))
        .unwrap()
    };
    assert_eq!(select(&die, &snapshot), (time(20), note.into()));
    for (started, finished) in [
      ("1970-01-01T00:00:02Z", "1970-01-01T00:00:04Z"),
      ("1970-01-01T00:00:21Z", "1970-01-01T00:00:25Z"),
      (start, "1970-01-01T00:00:14Z"),
    ] {
      let mut other = snapshot.clone();
      let state = other.data.state.as_mut().unwrap();
      state.started_at = Some(started.into());
      state.finished_at = Some(finished.into());
      assert_eq!(
        select(&die, &other),
        (time(30), "app: exited with code 137".into())
      );
    }
    die
      .metadata
      .as_mut()
      .unwrap()
      .as_object_mut()
      .unwrap()
      .remove("OOMKilled");
    die.note = Some(
      "VM guest stopped unexpectedly when process app exited (exit code 137)"
        .into(),
    );
    assert_ordinary_exit_keeps_event_time(&die, &snapshot);
    snapshot.data.state.as_mut().unwrap().oom_killed = Some(true);
    assert_eq!(
      select(&die, &snapshot),
      (
        time(20),
        format!("{} (OOM killed)", die.note.as_ref().unwrap())
      )
    );
  }

  #[test]
  fn status_failure_detail_selects_newest_informative_reason() {
    let observed = Some((time(10), "app: exited with code 127".into()));
    for (when, expected) in [
      (
        20,
        Some((
          time(20),
          "Job backup failed: Step 1 exited with code 7".into(),
        )),
      ),
      (5, observed.clone()),
    ] {
      assert_eq!(
        failure_detail(
          Some(event(
            when,
            "fail",
            "state_sync",
            "Job backup failed: Step 1 exited with code 7"
          )),
          observed.clone()
        ),
        expected
      );
    }
    assert_eq!(
      failure_detail(
        Some(event(20, "start", "image_pull", "image unavailable")),
        observed.clone()
      ),
      Some((time(20), "start: image_pull: image unavailable".into()))
    );
    assert_eq!(
      failure_detail(
        Some(event(5, "start", "image_pull", "older failure")),
        observed.clone()
      ),
      observed
    );
    assert_eq!(failure_detail(None, None), None);
  }

  #[test]
  fn status_render_keeps_missing_and_unreported_health_explicit() {
    assert!(
      render(&[], time(10), false, true, 80)
        .contains("No cargoes, VMs, jobs, or resources declared.")
    );
    let missing = empty_row(&EventActorKind::Cargo, "global.app");
    let mut resource = empty_row(&EventActorKind::Resource, "route");
    resource.status = "present".into();
    resource.health = "not reported".into();
    let output = render(&[missing, resource], time(10), true, true, 80);
    for expected in [
      "global.app",
      "missing",
      "unknown",
      "route",
      "present",
      "not reported",
      "RECENT FAILURE",
      "Ctrl-C",
      "UTC",
    ] {
      assert!(output.contains(expected), "missing {expected}");
    }
    assert!(!output.contains('\x1b'));
  }

  #[test]
  fn status_compact_render_fits_terminal_and_keeps_failure_details_near_object()
  {
    let mut cargo = empty_row(&EventActorKind::Cargo, "production.worker");
    set_runtime(
      &mut cargo,
      &ObjPsStatus {
        actual: ObjPsStatusKind::Fail,
        wanted: ObjPsStatusKind::Start,
        health: ObjPsHealthStatusKind::Unhealthy,
        ..Default::default()
      },
      &[instance(Some(false))],
    );
    cargo.recent_failure = Some((
      time(800),
      "production.worker-r0.c: unhealthy (health check exited with code 127)"
        .into(),
    ));
    let mut job = empty_row(&EventActorKind::Job, "migrate");
    job.state = "completed".into();
    job.running = "0/2".into();
    job.results = Some("2 succeeded, 0 failed".into());
    let mut resource = empty_row(&EventActorKind::Resource, "web-route");
    resource.state = "present".into();
    resource.health = "not reported".into();
    let rows = [cargo, job, resource];
    for width in [24, 80, 100] {
      let output = render(&rows, time(920), true, false, width);
      for line in output.lines() {
        assert!(
          measure_text_width(line) <= width,
          "line exceeds {width} columns: {line:?}"
        );
      }
      let words = output.split_whitespace().collect::<Vec<_>>().join(" ");
      if width >= 80 {
        assert!(words.contains("OBJECT STATE PROC HEALTH"));
      } else {
        assert!(output.contains("State:"));
      }
      for expected in [
        "failed",
        "Wanted: running",
        "Last failure 2m ago:",
        "health check exited with code 127",
        "completed",
        "Results: 2 succeeded, 0 failed",
        "not reported",
        "running/observed processes, including init, sandbox and sidecars",
      ] {
        assert!(words.contains(expected), "missing {expected}: {output}");
      }
      assert!(
        output.find("Last failure").unwrap() < output.find("job/").unwrap()
      );
      assert!(!output.contains("fail/start"));
    }
  }

  #[test]
  fn status_compact_render_preserves_long_unicode_identity() {
    let name = format!("production.{}.last", "数据库".repeat(80));
    let row = empty_row(&EventActorKind::Cargo, &name);
    assert_eq!(row.name, name);
    for width in [24, 80, 100] {
      let output =
        render(std::slice::from_ref(&row), time(920), false, false, width);
      assert!(output.lines().all(|line| measure_text_width(line) <= width));
      for character in ['数', '据', '库'] {
        assert_eq!(output.chars().filter(|c| *c == character).count(), 80);
      }
      assert!(!output.contains("..."));
    }
    let output = render(&[row], time(920), false, false, 24);
    let start = output.find("cargo/production.").unwrap();
    let end = output[start..].find("  State:").unwrap() + start;
    let identity = output[start..end].split_whitespace().collect::<String>();
    assert_eq!(identity, format!("cargo/{name}"));
  }

  #[test]
  fn status_render_keeps_distinct_whitespace_in_resource_names() {
    let names = ["route  blue", "route blue"];
    let rows = names
      .iter()
      .map(|name| empty_row(&EventActorKind::Resource, name))
      .collect::<Vec<_>>();
    for (row, name) in rows.iter().zip(names) {
      assert_eq!(row.name, name);
    }
    for wide in [false, true] {
      for width in [24, 80, 100] {
        let output = render(&rows, time(920), false, wide, width);
        for name in names {
          assert_eq!(output.matches(name).count(), 1, "{output}");
        }
        if !wide {
          assert!(output.lines().all(|line| measure_text_width(line) <= width));
        }
      }
    }
  }

  #[test]
  fn status_render_preserves_full_wrapped_identity_whitespace() {
    let name = format!("  {}  ", "route  数据库 ".repeat(8));
    let row = empty_row(&EventActorKind::Resource, &name);
    assert_eq!(row.name, name);
    let wide = render(std::slice::from_ref(&row), time(920), false, true, 80);
    assert!(wide.contains(&name));
    let output = render(&[row], time(920), false, false, 24);
    let start = output.find("resource/").unwrap();
    let end = output[start..].find("\n  State:").unwrap() + start;
    let identity = output[start..end].replace('\n', "");
    assert_eq!(identity, format!("resource/{name}"));
    assert!(output.lines().all(|line| measure_text_width(line) <= 24));
    assert_eq!(sanitize_identity("route  blue\n\t"), "route  blue  ");
  }

  #[test]
  fn status_compact_render_distinguishes_history_errors_and_current_state() {
    let mut running = empty_row(&EventActorKind::Cargo, "global.web");
    set_runtime(
      &mut running,
      &ObjPsStatus {
        actual: ObjPsStatusKind::Start,
        wanted: ObjPsStatusKind::Start,
        health: ObjPsHealthStatusKind::Healthy,
        ..Default::default()
      },
      &[instance(Some(true))],
    );
    running.read_failed = true;
    running.read_error =
      Some("event history unavailable: daemon read timed out".into());
    running.recent_failure =
      Some((time(800), "app: exited with code 127".into()));
    let missing = empty_row(&EventActorKind::Vm, "global.missing");
    let mut unavailable = empty_row(&EventActorKind::Job, "unavailable");
    unavailable.state = "unavailable".into();
    unavailable.read_failed = true;
    unavailable.read_error = Some("daemon read failed (HTTP 503)".into());
    let output = render(
      &[running, missing, unavailable],
      time(920),
      false,
      false,
      80,
    );
    for expected in [
      "running",
      "healthy",
      "Last failure 2m ago: app: exited with code 127",
      "event history unavailable: daemon read timed out",
      "missing",
      "unknown",
      "unavailable",
      "daemon read failed (HTTP 503)",
    ] {
      assert!(output.contains(expected), "missing {expected}: {output}");
    }
    assert!(!output.contains("Wanted:"));
  }

  #[test]
  fn status_wide_render_retains_actual_wanted_and_absolute_failure_time() {
    let mut row = empty_row(&EventActorKind::Job, "migrate");
    row.status = "finish/create (2 succeeded, 0 failed)".into();
    row.failure = "1970-01-01 00:13:20 UTC app: exited with code 127".into();
    let output = render(&[row], time(920), false, true, 24);
    assert!(output.contains("finish/create (2 succeeded, 0 failed)"));
    assert!(output.contains("1970-01-01 00:13:20 UTC"));
    assert!(output.contains("RECENT FAILURE"));
  }

  #[test]
  fn status_display_text_removes_controls_and_bounds_unicode() {
    assert_eq!(display_text("one\n\t two\r\0three\x1b"), "one two three");
    assert_eq!(
      display_text(&"é".repeat(201)),
      format!("{}...", "é".repeat(200))
    );
    assert_eq!(display_text(&"é".repeat(200)), "é".repeat(200));
  }

  #[test]
  fn status_process_failure_selects_newest_observation_within_window() {
    let old = process(
      "init",
      10,
      ContainerState {
        exit_code: Some(2),
        ..Default::default()
      },
    );
    let newest = process(
      "app",
      20,
      ContainerState {
        exit_code: Some(3),
        ..Default::default()
      },
    );
    let since = old.updated_at;
    let result = recent_failure(&[newest.clone(), old], since).unwrap();
    assert_eq!(
      result,
      (newest.updated_at, "app: exited with code 3".to_owned())
    );
    assert!(
      recent_failure(
        std::slice::from_ref(&newest),
        newest.updated_at + chrono::Duration::seconds(1)
      )
      .is_none()
    );
  }

  #[test]
  fn status_process_failure_prioritizes_oom_and_runtime_error() {
    let mut failed = process(
      "init",
      10,
      ContainerState {
        oom_killed: Some(true),
        error: Some("cannot start".to_owned()),
        exit_code: Some(137),
        ..Default::default()
      },
    );
    assert_eq!(
      recent_failure(&[failed.clone()], failed.updated_at)
        .unwrap()
        .1,
      "init: out of memory (OOM killed)"
    );
    failed.data.state.as_mut().unwrap().oom_killed = Some(false);
    assert_eq!(
      recent_failure(&[failed.clone()], failed.updated_at)
        .unwrap()
        .1,
      "init: cannot start"
    );
  }

  #[test]
  fn status_process_failure_ignores_success_and_previous_running_exit() {
    for (running, exit_code) in [(true, 7), (false, 0)] {
      let healthy = process(
        "app",
        10,
        ContainerState {
          running: Some(running),
          exit_code: Some(exit_code),
          error: Some("  ".to_owned()),
          ..Default::default()
        },
      );
      assert!(
        recent_failure(std::slice::from_ref(&healthy), healthy.updated_at)
          .is_none()
      );
    }
  }

  #[test]
  fn status_process_failure_uses_last_failed_check_without_log_output() {
    let unhealthy = process(
      "optional",
      10,
      ContainerState {
        running: Some(true),
        health: Some(Health {
          status: Some(HealthStatusEnum::UNHEALTHY),
          log: Some(
            vec![1, 2, 0]
              .into_iter()
              .map(|code| HealthcheckResult {
                exit_code: Some(code),
                output: Some("private check output".to_owned()),
                ..Default::default()
              })
              .collect(),
          ),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
    assert_eq!(
      recent_failure(std::slice::from_ref(&unhealthy), unhealthy.updated_at)
        .unwrap()
        .1,
      "optional: unhealthy (health check exited with code 2)"
    );
  }
}
