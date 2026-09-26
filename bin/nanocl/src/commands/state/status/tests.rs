use bollard_next::models::{ContainerInspectResponse, ContainerState};
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
