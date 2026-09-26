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
  let (layer, status, bytes, error) =
    if event.kind == EventKind::Error || error.is_some() {
      (
        state.and_then(|state| state.id.as_deref()),
        "failed",
        None,
        error.or(event.note.as_deref()),
      )
    } else if event.action == "download" {
      (None, "downloaded", None, None)
    } else {
      let state = state?;
      (
        state.id.as_deref(),
        state.status.as_deref().unwrap_or("Pulling image"),
        state
          .progress_detail
          .as_ref()
          .filter(|detail| detail.current.is_some_and(|current| current >= 0))
          .and_then(|_| byte_progress(state)),
        None,
      )
    };
  Some(StateOutputEvent::Image {
    resource,
    image,
    node: &event.reporting_node,
    layer,
    status,
    current: bytes.map(|(current, _)| current),
    total: bytes.map(|(_, total)| total),
    error,
  })
}

fn clear_layers(
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
  let layer = state.id.as_deref().unwrap_or_default();
  let bar = layers
    .entry((
      event.reporting_node.clone(),
      image.to_owned(),
      layer.to_owned(),
    ))
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
  bar.set_message(format!("{status} ({})", event.reporting_node));
  let template = if let Some((current, total)) = byte_progress(&state) {
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

/// Observe image events while the state operation remains authoritative.
pub async fn with_image_progress<T>(
  client: &NanocldClient,
  key: &str,
  kind: EventActorKind,
  progress: &MultiProgress,
  summary: &ProgressBar,
  output: Option<&StateOutput>,
  operation: impl Future<Output = IoResult<T>>,
) -> IoResult<T> {
  if output.is_none() && progress.is_hidden() {
    return operation.await;
  }
  // Event conditions stop the stream; they do not filter subscriptions.
  // Subscribe before polling the operation so its first pull events are seen.
  let Ok(response) = client
    .send_post("/events/watch", None::<Vec<EventCondition>>, None::<String>)
    .await
  else {
    return operation.await;
  };
  // Own the response stream directly so completing an operation closes its
  // subscription without leaving a background reader waiting for more events.
  let mut events = response;
  let mut buffer = Vec::new();
  let mut layers = HashMap::new();
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
          if let Some(output) = output {
            let state = image_state(&event);
            if let Some(event) =
              image_output(&event, key, &kind, &resource, state.as_ref())
              && let Err(err) = output.emit(event)
            {
              break 'observe Some(Err(err));
            }
          } else {
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
  }
}
