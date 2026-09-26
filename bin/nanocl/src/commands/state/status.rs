//! Read-only, Statefile-scoped runtime monitoring.

use std::{
  collections::BTreeSet,
  io::{self, IsTerminal, Write},
  time::Duration,
};

use bollard_next::models::HealthStatusEnum;
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
  Ok((since, recent_failure(&instances, since)))
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
mod tests {
  use bollard_next::models::{
    ContainerInspectResponse, ContainerState, Health, HealthcheckResult,
  };
  use nanocl_error::http::HttpError;
  use nanocld_client::stubs::{
    cargo_spec::CargoSpec,
    job::{JobPartial, JobSpec},
    process::ProcessKind,
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
    let filter = failure_filter(&EventActorKind::Job, "migrate", since);
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
    assert_eq!(row.health, "unhealthy");
  }

  #[test]
  fn status_job_distinguishes_schedule_from_completed_runs() {
    let mut job = JobInspect {
      created_at: time(10),
      updated_at: time(10),
      status: Default::default(),
      instance_total: 0,
      instance_success: 0,
      instance_running: 0,
      instance_failed: 0,
      spec: JobSpec {
        name: "migrate".into(),
        schedule: Some("@daily".into()),
        ..Default::default()
      },
      instances: vec![],
    };
    let mut row = empty_row(&EventActorKind::Job, "migrate");
    set_job(&mut row, &job);
    assert_eq!(row.status, "scheduled");
    assert_eq!(row.running, "0/0");
    assert_eq!(row.health, "unknown");
    job.status.actual = ObjPsStatusKind::Finish;
    job.instance_success = 2;
    set_job(&mut row, &job);
    assert_eq!(row.status, "finish/create (2 succeeded, 0 failed)");
    job.status.actual = ObjPsStatusKind::Fail;
    job.instance_failed = 1;
    set_job(&mut row, &job);
    assert_eq!(row.status, "fail/create (2 succeeded, 1 failed)");
    job.spec.schedule = None;
    job.status.actual = ObjPsStatusKind::Create;
    job.instance_success = 0;
    job.instance_failed = 0;
    set_job(&mut row, &job);
    assert_eq!(row.status, "create/create");
  }

  #[test]
  fn status_failure_detail_preserves_process_reason_for_generic_events() {
    for reason in [
      "app: exited with code 127",
      "app: out of memory (OOM killed)",
    ] {
      let observed = Some((time(10), reason.into()));
      assert_eq!(
        failure_detail(
          Some(event(20, "fail", "state_sync", "Job migrate")),
          observed.clone()
        ),
        observed
      );
    }
    let observed = Some((
      time(10),
      "app: unhealthy (health check exited with code 2)".into(),
    ));
    assert_eq!(
      failure_detail(
        Some(event(20, "unhealthy", "state_sync", "Cargo app")),
        observed.clone()
      ),
      observed
    );
  }

  #[test]
  fn status_failure_detail_selects_newest_informative_reason() {
    let observed = Some((time(10), "app: exited with code 127".into()));
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
      render(&[], time(10), false)
        .contains("No cargoes, VMs, jobs, or resources declared.")
    );
    let missing = empty_row(&EventActorKind::Cargo, "global.app");
    let mut resource = empty_row(&EventActorKind::Resource, "route");
    resource.status = "present".into();
    resource.health = "not reported".into();
    let output = render(&[missing, resource], time(10), true);
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
