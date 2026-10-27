use std::{collections::HashMap, future::Future, time::Duration};

use bollard_next::models::CreateImageInfo;
use futures::{StreamExt, future::Either};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use nanocl_error::io::IoResult;
use nanocld_client::{
  NanocldClient,
  stubs::system::{Event, EventActorKind, EventCondition, EventKind},
};

use crate::models::{StateOutput, StateOutputEvent};

fn related_image<'a>(
  event: &'a Event,
  key: &str,
  kind: &EventActorKind,
) -> Option<&'a str> {
  let related = event.related.as_ref()?;
  let actor = event.actor.as_ref()?;
  if related.key.as_deref() != Some(key)
    || &related.kind != kind
    || actor.kind != EventActorKind::ContainerImage
  {
    return None;
  }
  actor.key.as_deref()
}

fn image_state(event: &Event) -> Option<CreateImageInfo> {
  serde_json::from_value(event.metadata.as_ref()?.get("state")?.clone()).ok()
}

/// These are observations, not completion signals. In particular, the daemon
/// can still roll back after a Cargo healthy event during route handoff.
fn rollout_phase(
  event: &Event,
  key: &str,
  kind: &EventActorKind,
) -> Option<&'static str> {
  if kind != &EventActorKind::Cargo
    || event.reporting_controller != "nanocl.io/core"
    || event.reason != "state_sync"
  {
    return None;
  }
  let actor = event.actor.as_ref()?;
  if actor.kind == EventActorKind::Cargo && actor.key.as_deref() == Some(key) {
    if event.kind == EventKind::Error {
      return Some("observed cargo failure");
    }
    if event.kind != EventKind::Normal {
      return None;
    }
    return match event.action.as_str() {
      "starting" => Some("observed cargo starting"),
      "updating" => Some("observed cargo updating"),
      "start" => Some("observed cargo started"),
      "healthy" => Some("observed cargo healthy"),
      "unhealthy" => Some("observed cargo unhealthy"),
      "fail" => Some("observed cargo failure"),
      "update" => Some("observed cargo updated"),
      _ => None,
    };
  }
  let related = event.related.as_ref()?;
  if actor.kind != EventActorKind::Process
    || related.kind != EventActorKind::Cargo
    || related.key.as_deref() != Some(key)
    || event.kind != EventKind::Normal
  {
    return None;
  }
  let name = actor.key.as_deref()?;
  let attributes = actor.attributes.as_ref()?;
  let label = |key: &str| attributes.get(key).and_then(|value| value.as_str());
  let ordinal = label("io.nanocl.cargo.replica-ordinal")?
    .parse::<u32>()
    .ok()?;
  uuid::Uuid::parse_str(label("io.nanocl.cargo.replica")?).ok()?;
  label("io.nanocl.cargo.essential")?.parse::<bool>().ok()?;
  if label("io.nanocl") != Some("enabled")
    || label("io.nanocl.kind") != Some("cargo")
    || label("io.nanocl.c") != Some(key)
    || label("name") != Some(name)
    || label("io.nanocl.cargo.container")?.is_empty()
    || !name.starts_with(&format!("{key}-r{ordinal}-"))
    || !name.ends_with(".candidate.c")
    || !name.bytes().all(|byte| {
      byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
    })
  {
    return None;
  }
  // Retained/active process events and legacy events without these identity
  // labels cannot establish anything about the pending candidate.
  let role = label("io.nanocl.cargo.role")?;
  match (role, event.action.as_str()) {
    ("init", "start") if name.ends_with(".init.candidate.c") => {
      Some("observed candidate init running")
    }
    ("app", "start") => Some("observed candidate app started"),
    ("sandbox", "start") => Some("observed candidate sandbox started"),
    ("init", "die") if name.ends_with(".init.candidate.c") => {
      match label("exitCode")?.parse::<i64>().ok()? {
        0 => Some("observed candidate init exit zero"),
        _ => Some("observed candidate init failed"),
      }
    }
    ("app", "die") => Some("observed candidate app exited"),
    ("sandbox", "die") => Some("observed candidate sandbox exited"),
    ("init" | "app" | "sandbox", "oom") => {
      Some("observed candidate out of memory")
    }
    ("app", "update") => {
      // Docker health transitions are emitted as update events. Accept only
      // the daemon's exact note format; never render arbitrary event notes.
      match event
        .note
        .as_deref()?
        .strip_prefix(&format!("process {name} health_status: "))?
      {
        "starting" => Some("observed candidate health starting"),
        "healthy" => Some("observed candidate healthy"),
        "unhealthy" => Some("observed candidate health unhealthy"),
        _ => None,
      }
    }
    _ => None,
  }
}

fn rollout_output<'a>(
  resource: &'a str,
  phase: &'static str,
  summary: &ProgressBar,
  item: &ProgressBar,
) -> StateOutputEvent<'a> {
  StateOutputEvent::Item {
    resource,
    status: phase,
    completed: summary.position(),
    total: summary.length().unwrap_or(0),
    success: None,
    elapsed_ms: item.elapsed().as_millis() as u64,
    error: None,
  }
}

fn next_rollout_phase(
  event: &Event,
  key: &str,
  kind: &EventActorKind,
  previous: &mut Option<(String, EventActorKind, String, &'static str)>,
) -> Option<&'static str> {
  let phase = rollout_phase(event, key, kind)?;
  let actor = event.actor.as_ref()?;
  let actor_key = actor.key.as_deref()?;
  if previous.as_ref().is_some_and(|(node, kind, key, last)| {
    node == &event.reporting_node
      && kind == &actor.kind
      && key == actor_key
      && *last == phase
  }) {
    return None;
  }
  *previous = Some((
    event.reporting_node.clone(),
    actor.kind.clone(),
    actor_key.to_owned(),
    phase,
  ));
  Some(phase)
}

fn plain_rollout_line(
  resource: &str,
  phase: &str,
  elapsed: Duration,
) -> String {
  format!(
    "  {resource}  {phase} [{}; awaiting operation completion]",
    super::progress::plain_elapsed(elapsed)
  )
}

fn byte_progress(state: &CreateImageInfo) -> Option<(u64, u64)> {
  let detail = state.progress_detail.as_ref()?;
  let total = u64::try_from(detail.total?)
    .ok()
    .filter(|total| *total > 0)?;
  let current = u64::try_from(detail.current.unwrap_or_default())
    .unwrap_or_default()
    .min(total);
  Some((current, total))
}

fn image_output<'a>(
  event: &'a Event,
  key: &str,
  kind: &EventActorKind,
  resource: &'a str,
  state: Option<&'a CreateImageInfo>,
) -> Option<StateOutputEvent<'a>> {
  let image = related_image(event, key, kind)?;
  if !matches!(event.action.as_str(), "downloading" | "download") {
    return None;
  }
  let error = state.and_then(|state| state.error.as_deref());
  let (status, error) = if event.kind == EventKind::Error || error.is_some() {
    (Some("failed"), error.or(event.note.as_deref()))
  } else if event.action == "download" {
    (Some("downloaded"), None)
  } else {
    state?;
    (None, None)
  };
  Some(image_output_record(
    resource,
    image,
    &event.reporting_node,
    state,
    status,
    error,
  ))
}

/// Format both direct Docker pulls and daemon image events consistently.
pub(crate) fn image_output_record<'a>(
  resource: &'a str,
  image: &'a str,
  node: &'a str,
  state: Option<&'a CreateImageInfo>,
  status: Option<&'a str>,
  error: Option<&'a str>,
) -> StateOutputEvent<'a> {
  let error = error.or_else(|| state.and_then(|state| state.error.as_deref()));
  let status = if error.is_some() {
    "failed"
  } else {
    status
      .or_else(|| state.and_then(|state| state.status.as_deref()))
      .unwrap_or("Pulling image")
  };
  let layer = if status == "downloaded" {
    None
  } else {
    state.and_then(|state| state.id.as_deref())
  };
  let bytes = state
    .filter(|_| !matches!(status, "failed" | "downloaded"))
    .filter(|state| {
      state.progress_detail.as_ref().is_some_and(|detail| {
        detail.current.is_some_and(|current| current >= 0)
      })
    })
    .and_then(byte_progress);
  StateOutputEvent::Image {
    resource,
    image,
    node,
    layer,
    status,
    current: bytes.map(|(current, _)| current),
    total: bytes.map(|(_, total)| total),
    error,
  }
}

pub(crate) fn clear_layers(
  progress: &MultiProgress,
  layers: &mut HashMap<(String, String, String), ProgressBar>,
  image: Option<(&str, &str)>,
) {
  layers.retain(|(node, name, _), bar| {
    if image.is_none_or(|image| image == (node.as_str(), name.as_str())) {
      bar.finish_and_clear();
      progress.remove(bar);
      false
    } else {
      true
    }
  });
}

fn update_image_progress(
  event: &Event,
  key: &str,
  kind: &EventActorKind,
  progress: &MultiProgress,
  summary: &ProgressBar,
  layers: &mut HashMap<(String, String, String), ProgressBar>,
) {
  let Some(image) = related_image(event, key, kind) else {
    return;
  };
  if !matches!(event.action.as_str(), "downloading" | "download") {
    return;
  }
  if event.action == "download" || event.kind == EventKind::Error {
    clear_layers(progress, layers, Some((&event.reporting_node, image)));
    return;
  }
  let Some(state) = image_state(event) else {
    return;
  };
  render_image_progress(
    &event.reporting_node,
    image,
    &state,
    progress,
    summary,
    layers,
  );
}

/// Render a pull layer using the same layout for every command.
pub(crate) fn render_image_progress(
  node: &str,
  image: &str,
  state: &CreateImageInfo,
  progress: &MultiProgress,
  summary: &ProgressBar,
  layers: &mut HashMap<(String, String, String), ProgressBar>,
) {
  let layer = state.id.as_deref().unwrap_or_default();
  let bar = layers
    .entry((node.to_owned(), image.to_owned(), layer.to_owned()))
    .or_insert_with(|| {
      let bar = progress.insert_before(summary, ProgressBar::new_spinner());
      bar.set_prefix(format!("{image} {layer}"));
      bar
    });
  let status = state
    .error
    .as_deref()
    .or(state.status.as_deref())
    .unwrap_or("Pulling image");
  bar.set_message(format!("{status} ({node})"));
  let template = if let Some((current, total)) = byte_progress(state) {
    bar.disable_steady_tick();
    bar.set_length(total);
    bar.set_position(current);
    "    {prefix:.dim} {msg} [{bar:16.cyan/blue}] {bytes}/{total_bytes}"
  } else {
    bar.set_length(0);
    bar.set_position(0);
    if matches!(
      status,
      "Already exists" | "Pull complete" | "Download complete"
    ) {
      bar.disable_steady_tick();
      "    {prefix:.dim} {msg}"
    } else {
      bar.enable_steady_tick(Duration::from_millis(100));
      "  {spinner:.cyan} {prefix:.dim} {msg}"
    }
  };
  bar.set_style(
    ProgressStyle::with_template(template)
      .expect("valid image progress style")
      .progress_chars("=> "),
  );
}

const MAX_IMAGE_EVENT_BYTES: usize = 1024 * 1024;

fn image_events(buffer: &mut Vec<u8>, bytes: &[u8]) -> Option<Vec<Event>> {
  let mut events = Vec::new();
  for part in bytes.split_inclusive(|byte| *byte == b'\n') {
    if buffer.len().saturating_add(part.len()) > MAX_IMAGE_EVENT_BYTES {
      return None;
    }
    buffer.extend_from_slice(part);
    if part.last() != Some(&b'\n') {
      continue;
    }
    if !buffer.iter().all(u8::is_ascii_whitespace) {
      events.push(serde_json::from_slice(buffer).ok()?);
    }
    buffer.clear();
  }
  Some(events)
}

/// Observe image and Cargo phases while the state operation remains authoritative.
/// Bound subscription setup so optional observation cannot hold up the operation.
pub async fn with_image_progress<T>(
  client: &NanocldClient,
  key: &str,
  kind: EventActorKind,
  progress: (&MultiProgress, &ProgressBar),
  item: &ProgressBar,
  output: Option<&StateOutput>,
  operation: impl Future<Output = IoResult<T>>,
) -> IoResult<T> {
  let (progress, summary) = progress;
  if output.is_none() && progress.is_hidden() && kind != EventActorKind::Cargo {
    return operation.await;
  }
  // Event conditions stop the stream; they do not filter subscriptions.
  // Subscribe before polling the operation so its first pull events are seen.
  let Ok(Ok(response)) = ntex::time::timeout(
    ntex::time::Millis(2_000),
    client.send_post(
      "/events/watch",
      None::<Vec<EventCondition>>,
      None::<String>,
    ),
  )
  .await
  else {
    return operation.await;
  };
  // Own the response stream directly so completing an operation closes its
  // subscription without leaving a background reader waiting for more events.
  let mut events = response;
  let mut buffer = Vec::new();
  let mut layers = HashMap::new();
  // Retain only the last observation: repeated identical events are quiet,
  // but recovery and the same phase on another candidate remain visible.
  let mut previous_observation = None;
  let resource = format!("{}/{key}", kind.to_string().to_lowercase());
  futures::pin_mut!(operation);
  let result = 'observe: loop {
    match futures::future::select(operation.as_mut(), events.next()).await {
      Either::Left((result, _)) => break Some(result),
      Either::Right((Some(Ok(bytes)), _)) => {
        let Some(events) = image_events(&mut buffer, &bytes) else {
          break None;
        };
        for event in events {
          if let Some(phase) =
            next_rollout_phase(&event, key, &kind, &mut previous_observation)
          {
            if let Some(output) = output {
              if let Err(err) =
                output.emit(rollout_output(&resource, phase, summary, item))
              {
                break 'observe Some(Err(err));
              }
            } else {
              let line = plain_rollout_line(&resource, phase, item.elapsed());
              if progress.is_hidden() {
                eprintln!("{line}");
              } else {
                let _ = progress.println(line);
              }
            }
          }
          if let Some(output) = output {
            let state = image_state(&event);
            if let Some(event) =
              image_output(&event, key, &kind, &resource, state.as_ref())
              && let Err(err) = output.emit(event)
            {
              break 'observe Some(Err(err));
            }
          } else if !progress.is_hidden() {
            update_image_progress(
              &event,
              key,
              &kind,
              progress,
              summary,
              &mut layers,
            );
          }
        }
      }
      Either::Right((_, _)) => break None,
    }
  };
  drop(events);
  clear_layers(progress, &mut layers, None);
  match result {
    Some(result) => result,
    None => operation.await,
  }
}

#[cfg(test)]
mod tests {
  use indicatif::ProgressDrawTarget;
  use nanocld_client::stubs::system::EventActor;
  use serde_json::json;

  use super::*;

  fn event() -> Event {
    Event {
      key: uuid::Uuid::nil(),
      created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
      expires_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
      reporting_node: "node-a".to_owned(),
      reporting_controller: "nanocl.io/core".to_owned(),
      kind: EventKind::Normal,
      action: "downloading".to_owned(),
      reason: "state_sync".to_owned(),
      note: None,
      actor: Some(EventActor {
        key: Some("alpine:latest".to_owned()),
        kind: EventActorKind::ContainerImage,
        attributes: None,
      }),
      related: Some(EventActor {
        key: Some("global.app".to_owned()),
        kind: EventActorKind::Cargo,
        attributes: None,
      }),
      metadata: Some(json!({"state": {
        "id": "layer-a",
        "status": "Downloading",
        "progressDetail": {"current": 25, "total": 100}
      }})),
    }
  }

  fn image_json(
    event: &Event,
    key: &str,
    kind: &EventActorKind,
  ) -> Option<serde_json::Value> {
    let state = image_state(event);
    let resource = format!("{}/{key}", kind.to_string().to_lowercase());
    image_output(event, key, kind, &resource, state.as_ref())
      .map(|event| serde_json::to_value(event).unwrap())
  }

  fn cargo_event(action: &str) -> Event {
    let mut event = event();
    event.action = action.to_owned();
    event.actor = Some(EventActor {
      key: Some("global.app".to_owned()),
      kind: EventActorKind::Cargo,
      attributes: Some(json!({"private": "not emitted"})),
    });
    event.related = None;
    event.metadata = None;
    event.note = Some("private event detail must not be emitted".to_owned());
    event
  }

  fn candidate_event(role: &str, action: &str) -> Event {
    let mut event = event();
    let suffix = if role == "init" { ".init" } else { "" };
    let name = format!("global.app-r0-api-abcdef{suffix}.candidate.c");
    event.action = action.to_owned();
    event.actor = Some(EventActor {
      key: Some(name.clone()),
      kind: EventActorKind::Process,
      attributes: Some(json!({
        "name": name,
        "io.nanocl": "enabled",
        "io.nanocl.kind": "cargo",
        "io.nanocl.c": "global.app",
        "io.nanocl.cargo.role": role,
        "io.nanocl.cargo.container": "api",
        "io.nanocl.cargo.replica": "7a9778c5-6e52-49a0-99f8-b63bb15a4337",
        "io.nanocl.cargo.replica-ordinal": "0",
        "io.nanocl.cargo.essential": "true",
        "private": "not emitted"
      })),
    });
    event.metadata = None;
    event
  }

  #[test]
  fn rollout_observations_do_not_complete_or_advance_the_operation() {
    let event = cargo_event("healthy");
    let phase =
      rollout_phase(&event, "global.app", &EventActorKind::Cargo).unwrap();
    let summary = ProgressBar::hidden();
    summary.set_length(3);
    summary.set_position(1);
    let item = ProgressBar::hidden().with_elapsed(Duration::from_secs(2));
    let before = item.elapsed().as_millis() as u64;
    let output = serde_json::to_value(rollout_output(
      "cargo/global.app",
      phase,
      &summary,
      &item,
    ))
    .unwrap();
    let elapsed = output["elapsed_ms"].as_u64().unwrap();
    assert!((before..=item.elapsed().as_millis() as u64).contains(&elapsed));
    assert_eq!(
      output,
      json!({
        "type": "item", "resource": "cargo/global.app",
        "status": "observed cargo healthy", "completed": 1,
        "total": 3, "elapsed_ms": elapsed
      })
    );
    assert_eq!(summary.position(), 1);
    assert!(!summary.is_finished());
    assert!(!output.to_string().contains("private"));
    for action in ["starting", "updating", "start", "update", "unhealthy"] {
      assert!(
        rollout_phase(
          &cargo_event(action),
          "global.app",
          &EventActorKind::Cargo
        )
        .unwrap()
        .starts_with("observed cargo ")
      );
    }
    let mut failed = cargo_event("updating");
    failed.kind = EventKind::Error;
    assert_eq!(
      rollout_phase(&failed, "global.app", &EventActorKind::Cargo),
      Some("observed cargo failure")
    );
  }

  #[test]
  fn rollout_observations_suppress_only_consecutive_identical_phases() {
    let mut previous = None;
    let mut event = candidate_event("app", "update");
    let name = event
      .actor
      .as_ref()
      .unwrap()
      .key
      .as_deref()
      .unwrap()
      .to_owned();
    for (health, expected) in [
      ("healthy", "observed candidate healthy"),
      ("unhealthy", "observed candidate health unhealthy"),
      ("healthy", "observed candidate healthy"),
    ] {
      event.note = Some(format!("process {name} health_status: {health}"));
      assert_eq!(
        next_rollout_phase(
          &event,
          "global.app",
          &EventActorKind::Cargo,
          &mut previous
        ),
        Some(expected)
      );
      assert!(
        next_rollout_phase(
          &event,
          "global.app",
          &EventActorKind::Cargo,
          &mut previous
        )
        .is_none()
      );
    }
  }

  #[test]
  fn rollout_observations_preserve_distinct_candidates_and_nodes() {
    let first = candidate_event("app", "start");
    let mut second = first.clone();
    let actor = second.actor.as_mut().unwrap();
    let name = "global.app-r1-api-ghijkl.candidate.c";
    actor.key = Some(name.to_owned());
    let attributes = actor.attributes.as_mut().unwrap();
    attributes["name"] = json!(name);
    attributes["io.nanocl.cargo.replica-ordinal"] = json!("1");
    attributes["io.nanocl.cargo.replica"] =
      json!("edb8fabc-4621-4e88-9a65-cd12b14359d2");
    let mut another_node = second.clone();
    another_node.reporting_node = "node-b".to_owned();
    let mut previous = None;
    for event in [&first, &second, &another_node, &first] {
      assert_eq!(
        next_rollout_phase(
          event,
          "global.app",
          &EventActorKind::Cargo,
          &mut previous
        ),
        Some("observed candidate app started")
      );
    }
  }

  #[test]
  fn rollout_observations_use_the_item_clock_and_shared_elapsed_format() {
    let summary = ProgressBar::hidden();
    let item = ProgressBar::hidden();
    for elapsed in [Duration::from_secs(62), Duration::from_secs(3723)] {
      item.set_elapsed(elapsed);
      let before = item.elapsed().as_millis() as u64;
      let output = serde_json::to_value(rollout_output(
        "cargo/global.app",
        "observed cargo healthy",
        &summary,
        &item,
      ))
      .unwrap();
      let observed = output["elapsed_ms"].as_u64().unwrap();
      assert!((before..=item.elapsed().as_millis() as u64).contains(&observed));
    }
    for (seconds, formatted) in [(62, "1m 02s"), (3723, "1h 02m 03s")] {
      assert_eq!(
        plain_rollout_line(
          "cargo/global.app",
          "observed cargo healthy",
          Duration::from_secs(seconds)
        ),
        format!(
          "  cargo/global.app  observed cargo healthy [{formatted}; awaiting operation completion]"
        )
      );
    }
  }

  #[test]
  fn rollout_observations_reject_unrelated_cargo_events() {
    let mut event = cargo_event("healthy");
    assert!(
      rollout_phase(&event, "other.app", &EventActorKind::Cargo).is_none()
    );
    assert!(rollout_phase(&event, "global.app", &EventActorKind::Vm).is_none());
    event.reason = "external".to_owned();
    assert!(
      rollout_phase(&event, "global.app", &EventActorKind::Cargo).is_none()
    );
    event.reason = "state_sync".to_owned();
    event.reporting_controller = "another/controller".to_owned();
    assert!(
      rollout_phase(&event, "global.app", &EventActorKind::Cargo).is_none()
    );
    event.reporting_controller = "nanocl.io/core".to_owned();
    event.kind = EventKind::Warning;
    assert!(
      rollout_phase(&event, "global.app", &EventActorKind::Cargo).is_none()
    );
  }

  #[test]
  fn rollout_observations_accept_only_exact_candidate_health_and_init_status() {
    let mut event = candidate_event("app", "start");
    assert_eq!(
      rollout_phase(&event, "global.app", &EventActorKind::Cargo),
      Some("observed candidate app started")
    );
    event.action = "update".to_owned();
    let name = event.actor.as_ref().unwrap().key.as_ref().unwrap().clone();
    for (health, phase) in [
      ("starting", "observed candidate health starting"),
      ("healthy", "observed candidate healthy"),
      ("unhealthy", "observed candidate health unhealthy"),
    ] {
      event.note = Some(format!("process {name} health_status: {health}"));
      assert_eq!(
        rollout_phase(&event, "global.app", &EventActorKind::Cargo),
        Some(phase)
      );
    }
    for note in [
      "health_status: healthy".to_owned(),
      "process old-process health_status: healthy".to_owned(),
      format!("process {name} health_status: healthy\nprivate"),
    ] {
      event.note = Some(note);
      assert!(
        rollout_phase(&event, "global.app", &EventActorKind::Cargo).is_none()
      );
    }
    let mut init = candidate_event("init", "start");
    assert_eq!(
      rollout_phase(&init, "global.app", &EventActorKind::Cargo),
      Some("observed candidate init running")
    );
    init.action = "die".to_owned();
    for (code, phase) in [
      ("0", Some("observed candidate init exit zero")),
      ("1", Some("observed candidate init failed")),
      ("unknown", None),
    ] {
      init.actor.as_mut().unwrap().attributes.as_mut().unwrap()["exitCode"] =
        json!(code);
      assert_eq!(
        rollout_phase(&init, "global.app", &EventActorKind::Cargo),
        phase
      );
    }
  }

  #[test]
  fn rollout_observations_reject_retained_and_uncorrelated_processes() {
    let event = candidate_event("app", "start");
    for name in [
      "global.app-r0-api-abcdef.tmp.c",
      "global.app-r0-api-abcdef.c",
      "other.app-r0-api-abcdef.candidate.c",
      "global.app-r1-api-abcdef.candidate.c",
    ] {
      let mut changed = event.clone();
      let actor = changed.actor.as_mut().unwrap();
      actor.key = Some(name.to_owned());
      actor.attributes.as_mut().unwrap()["name"] = json!(name);
      assert!(
        rollout_phase(&changed, "global.app", &EventActorKind::Cargo).is_none()
      );
    }
    for label in [
      "io.nanocl",
      "io.nanocl.kind",
      "io.nanocl.c",
      "name",
      "io.nanocl.cargo.replica",
      "io.nanocl.cargo.replica-ordinal",
      "io.nanocl.cargo.container",
      "io.nanocl.cargo.role",
      "io.nanocl.cargo.essential",
    ] {
      let mut changed = event.clone();
      changed
        .actor
        .as_mut()
        .unwrap()
        .attributes
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove(label);
      assert!(
        rollout_phase(&changed, "global.app", &EventActorKind::Cargo).is_none()
      );
    }
    for (label, value) in [
      ("io.nanocl.kind", "vm"),
      ("io.nanocl.c", "other.app"),
      ("io.nanocl.cargo.replica", "not-a-uuid"),
      ("io.nanocl.cargo.essential", "unknown"),
      ("io.nanocl.cargo.container", ""),
      ("io.nanocl.cargo.role", "unknown"),
    ] {
      let mut changed = event.clone();
      changed.actor.as_mut().unwrap().attributes.as_mut().unwrap()[label] =
        json!(value);
      assert!(
        rollout_phase(&changed, "global.app", &EventActorKind::Cargo).is_none()
      );
    }
    let mut changed = event.clone();
    changed.related.as_mut().unwrap().key = Some("other.app".to_owned());
    assert!(
      rollout_phase(&changed, "global.app", &EventActorKind::Cargo).is_none()
    );
  }

  #[test]
  fn image_progress_json_filters_and_limits_payload() {
    let mut event = event();
    event.actor.as_mut().unwrap().attributes =
      Some(json!({"private": "ignored"}));
    event.related.as_mut().unwrap().attributes =
      Some(json!({"private": "ignored"}));
    event.metadata.as_mut().unwrap()["private"] = json!("ignored");
    assert_eq!(
      image_json(&event, "global.app", &EventActorKind::Cargo),
      Some(json!({
        "type": "image",
        "resource": "cargo/global.app",
        "image": "alpine:latest",
        "node": "node-a",
        "layer": "layer-a",
        "status": "Downloading",
        "current": 25,
        "total": 100
      }))
    );
    assert!(image_json(&event, "other.app", &EventActorKind::Cargo).is_none());
    assert!(image_json(&event, "global.app", &EventActorKind::Job).is_none());
    event.action = "create".to_owned();
    assert!(image_json(&event, "global.app", &EventActorKind::Cargo).is_none());
    event.action = "downloading".to_owned();
    for (kind, resource) in [
      (EventActorKind::Job, "job/global.app"),
      (EventActorKind::Vm, "vm/global.app"),
    ] {
      event.related.as_mut().unwrap().kind = kind.clone();
      assert_eq!(
        image_json(&event, "global.app", &kind).unwrap()["resource"],
        resource
      );
    }
  }

  #[test]
  fn image_progress_json_reports_completion_errors_and_unknown_totals() {
    let mut event = event();
    event.action = "download".to_owned();
    event.metadata = None;
    assert_eq!(
      image_json(&event, "global.app", &EventActorKind::Cargo),
      Some(json!({
        "type": "image", "resource": "cargo/global.app", "image": "alpine:latest",
        "node": "node-a", "status": "downloaded"
      }))
    );
    event.action = "downloading".to_owned();
    event.kind = EventKind::Error;
    event.note = Some("image unavailable".to_owned());
    assert_eq!(
      image_json(&event, "global.app", &EventActorKind::Cargo),
      Some(json!({
        "type": "image", "resource": "cargo/global.app", "image": "alpine:latest",
        "node": "node-a", "status": "failed", "error": "image unavailable"
      }))
    );
    event.kind = EventKind::Normal;
    event.metadata =
      Some(json!({"state": {"id": "layer-a", "error": "pull failed"}}));
    let output =
      image_json(&event, "global.app", &EventActorKind::Cargo).unwrap();
    assert_eq!(output["status"], "failed");
    assert_eq!(output["error"], "pull failed");
    event.metadata = Some(
      json!({"state": {"id": "layer-a", "status": "Extracting", "progressDetail": {"current": 10}}}),
    );
    let output =
      image_json(&event, "global.app", &EventActorKind::Cargo).unwrap();
    assert_eq!(output["status"], "Extracting");
    assert!(output.get("current").is_none());
    assert!(output.get("total").is_none());
    assert!(output.get("error").is_none());
    event.metadata.as_mut().unwrap()["state"]["progressDetail"] =
      json!({"current": 150, "total": 100});
    let output =
      image_json(&event, "global.app", &EventActorKind::Cargo).unwrap();
    assert_eq!(output["current"], 100);
    assert_eq!(output["total"], 100);
    for detail in [json!({"total": 100}), json!({"current": -1, "total": 100})]
    {
      event.metadata.as_mut().unwrap()["state"]["progressDetail"] = detail;
      let output =
        image_json(&event, "global.app", &EventActorKind::Cargo).unwrap();
      assert!(output.get("current").is_none());
      assert!(output.get("total").is_none());
    }
  }

  #[test]
  fn image_progress_direct_pull_records_match_state_events() {
    let mut event = event();
    for (metadata, expected) in [
      (
        json!({"state": {
          "id": "layer-a", "status": "Downloading",
          "progressDetail": {"current": 25, "total": 100}
        }}),
        json!({
          "type": "image", "resource": "cargo/global.app",
          "image": "alpine:latest", "node": "node-a", "layer": "layer-a",
          "status": "Downloading", "current": 25, "total": 100
        }),
      ),
      (
        json!({"state": {"id": "layer-a", "status": "Already exists"}}),
        json!({
          "type": "image", "resource": "cargo/global.app",
          "image": "alpine:latest", "node": "node-a", "layer": "layer-a",
          "status": "Already exists"
        }),
      ),
      (
        json!({"state": {"id": "layer-a", "error": "pull failed"}}),
        json!({
          "type": "image", "resource": "cargo/global.app",
          "image": "alpine:latest", "node": "node-a", "layer": "layer-a",
          "status": "failed", "error": "pull failed"
        }),
      ),
    ] {
      event.metadata = Some(metadata);
      let state = image_state(&event).unwrap();
      let direct = image_output_record(
        "cargo/global.app",
        "alpine:latest",
        "node-a",
        Some(&state),
        None,
        None,
      );
      assert_eq!(serde_json::to_value(direct).unwrap(), expected);
      assert_eq!(
        image_json(&event, "global.app", &EventActorKind::Cargo).unwrap(),
        expected,
      );
    }
    event.action = "download".to_owned();
    event.metadata = None;
    let completed = image_output_record(
      "cargo/global.app",
      "alpine:latest",
      "node-a",
      None,
      Some("downloaded"),
      None,
    );
    let completed = serde_json::to_value(completed).unwrap();
    assert_eq!(
      completed,
      json!({
        "type": "image", "resource": "cargo/global.app",
        "image": "alpine:latest", "node": "node-a", "status": "downloaded"
      }),
    );
    assert_eq!(
      completed,
      image_json(&event, "global.app", &EventActorKind::Cargo).unwrap(),
    );
    event.kind = EventKind::Error;
    event.note = Some("stream failed".to_owned());
    let failed = image_output_record(
      "cargo/global.app",
      "alpine:latest",
      "node-a",
      None,
      Some("failed"),
      Some("stream failed"),
    );
    let failed = serde_json::to_value(failed).unwrap();
    assert_eq!(
      failed,
      json!({
        "type": "image", "resource": "cargo/global.app",
        "image": "alpine:latest", "node": "node-a",
        "status": "failed", "error": "stream failed"
      }),
    );
    assert_eq!(
      failed,
      image_json(&event, "global.app", &EventActorKind::Cargo).unwrap(),
    );
  }

  #[test]
  fn image_progress_decodes_split_and_coalesced_events() {
    let mut record = serde_json::to_vec(&event()).unwrap();
    record.push(b'\n');
    let split = record.len() / 2;
    let mut buffer = Vec::new();
    assert!(
      image_events(&mut buffer, &record[..split])
        .unwrap()
        .is_empty()
    );
    let mut chunk = record[split..].to_vec();
    chunk.extend_from_slice(&record);
    chunk.extend_from_slice(&record[..split]);
    let events = image_events(&mut buffer, &chunk).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].action, "downloading");
    assert_eq!(events[1].reporting_node, "node-a");
    assert_eq!(buffer, record[..split]);
    assert_eq!(
      image_events(&mut buffer, &record[split..]).unwrap().len(),
      1
    );
    assert!(buffer.is_empty());
    assert!(image_events(&mut buffer, b" \n\n").unwrap().is_empty());
    assert!(image_events(&mut buffer, b"{invalid}\n").is_none());
    buffer.clear();
    assert!(
      image_events(&mut buffer, &vec![b'a'; MAX_IMAGE_EVENT_BYTES + 1])
        .is_none()
    );
    assert!(buffer.is_empty());
  }

  #[test]
  fn image_progress_rejects_unrelated_actors() {
    let mut event = event();
    assert_eq!(
      related_image(&event, "global.app", &EventActorKind::Cargo),
      Some("alpine:latest")
    );
    assert!(
      related_image(&event, "other.app", &EventActorKind::Cargo).is_none()
    );
    assert!(
      related_image(&event, "global.app", &EventActorKind::Job).is_none()
    );
    event.related = None;
    assert!(
      related_image(&event, "global.app", &EventActorKind::Cargo).is_none()
    );
    event.related = Some(EventActor {
      key: Some("global.app".to_owned()),
      kind: EventActorKind::Cargo,
      attributes: None,
    });
    event.actor.as_mut().unwrap().kind = EventActorKind::Cargo;
    assert!(
      related_image(&event, "global.app", &EventActorKind::Cargo).is_none()
    );
  }

  #[test]
  fn image_progress_parses_and_bounds_byte_counts() {
    let mut event = event();
    assert_eq!(
      byte_progress(&image_state(&event).unwrap()),
      Some((25, 100))
    );
    for (detail, expected) in [
      (json!({"current": 150, "total": 100}), Some((100, 100))),
      (json!({"current": -1, "total": 100}), Some((0, 100))),
      (json!({"total": 100}), Some((0, 100))),
      (json!({"current": 25}), None),
      (json!({"current": 25, "total": 0}), None),
      (json!({"current": 25, "total": -1}), None),
    ] {
      event.metadata.as_mut().unwrap()["state"]["progressDetail"] = detail;
      assert_eq!(byte_progress(&image_state(&event).unwrap()), expected);
    }
    event.metadata = Some(json!({"state": {"status": "Already exists"}}));
    assert_eq!(byte_progress(&image_state(&event).unwrap()), None);
    event.metadata = Some(json!({"state": {"progressDetail": "invalid"}}));
    assert!(image_state(&event).is_none());
  }

  #[test]
  fn image_progress_tracks_phases_and_clears_only_completed_image() {
    let progress =
      MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
    let summary = progress.add(ProgressBar::new(1));
    let mut layers = HashMap::new();
    let mut event = event();
    let layer_key = (
      "node-a".to_owned(),
      "alpine:latest".to_owned(),
      "layer-a".to_owned(),
    );
    let update = |event: &Event, layers: &mut HashMap<_, _>| {
      update_image_progress(
        event,
        "global.app",
        &EventActorKind::Cargo,
        &progress,
        &summary,
        layers,
      );
    };
    update(&event, &mut layers);
    assert_eq!(layers[&layer_key].position(), 25);
    assert_eq!(layers[&layer_key].length(), Some(100));
    event.metadata = Some(json!({"state": {
      "id": "layer-a", "status": "Extracting",
      "progressDetail": {"current": 5, "total": 200}
    }}));
    update(&event, &mut layers);
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[&layer_key].position(), 5);
    assert_eq!(layers[&layer_key].length(), Some(200));
    assert!(layers[&layer_key].message().starts_with("Extracting"));
    event.metadata = Some(json!({"state": {
      "id": "layer-a", "status": "Already exists"
    }}));
    update(&event, &mut layers);
    assert_eq!(layers[&layer_key].length(), Some(0));
    assert!(layers[&layer_key].message().starts_with("Already exists"));
    let mut other_layer = event.clone();
    other_layer.metadata.as_mut().unwrap()["state"]["id"] = json!("layer-b");
    update(&other_layer, &mut layers);
    let mut other_node = event.clone();
    other_node.reporting_node = "node-b".to_owned();
    update(&other_node, &mut layers);
    let mut other_image = event.clone();
    other_image.actor.as_mut().unwrap().key = Some("busybox:latest".to_owned());
    update(&other_image, &mut layers);
    assert_eq!(layers.len(), 4);
    let completed = layers[&layer_key].clone();
    event.action = "download".to_owned();
    event.metadata = None;
    update(&event, &mut layers);
    assert_eq!(layers.len(), 2);
    assert!(!layers.contains_key(&layer_key));
    assert!(completed.is_finished());
    other_node.kind = EventKind::Error;
    update(&other_node, &mut layers);
    assert_eq!(layers.len(), 1);
    clear_layers(&progress, &mut layers, None);
    assert!(layers.is_empty());
    let state = image_state(&other_image).unwrap();
    render_image_progress(
      "node-a",
      "busybox:latest",
      &state,
      &progress,
      &summary,
      &mut layers,
    );
    assert_eq!(layers.len(), 1);
    let bar = layers.values().next().unwrap().clone();
    assert!(bar.message().starts_with("Already exists"));
    clear_layers(&progress, &mut layers, None);
    assert!(layers.is_empty());
    assert!(bar.is_finished());
  }
}
