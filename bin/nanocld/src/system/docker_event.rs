use ntex::rt;
use futures_util::StreamExt;

use nanocl_error::io::{IoResult, FromIo};

use bollard_next::{
  system::EventsOptions,
  container::InspectContainerOptions,
  service::{EventMessageTypeEnum, EventMessage},
};
use nanocl_stubs::system::{
  EventActorKind, NativeEventAction, EventActor, EventKind, EventPartial,
};

use crate::{
  vars,
  repositories::generic::*,
  models::{ProcessDb, ProcessUpdateDb, SystemState},
};

/// Take actions when a docker event is received
async fn exec_docker(
  event: &EventMessage,
  state: &SystemState,
) -> IoResult<()> {
  let kind = event.typ.unwrap_or(EventMessageTypeEnum::EMPTY);
  if kind != EventMessageTypeEnum::CONTAINER {
    return Ok(());
  }
  let actor = event.actor.clone().unwrap_or_default();
  let attributes = actor.attributes.unwrap_or_default();
  if attributes.get("io.nanocl").is_none() {
    return Ok(());
  }
  let Some(kind) = attributes.get("io.nanocl.kind") else {
    return Ok(());
  };
  let (kind, kind_key) = match kind.as_str() {
    "cargo" => (
      EventActorKind::Cargo,
      attributes.get("io.nanocl.c").cloned(),
    ),
    "vm" => (EventActorKind::Vm, attributes.get("io.nanocl.v").cloned()),
    "job" => (EventActorKind::Job, attributes.get("io.nanocl.j").cloned()),
    _ => {
      return Ok(());
    }
  };
  let Some(kind_key) = kind_key else {
    return Ok(());
  };
  let action = event.action.clone().unwrap_or_default();
  let id = actor.id.unwrap_or_default();
  let name = attributes.get("name").cloned().unwrap_or_default();
  let action = action.as_str();
  let mut event = EventPartial {
    reporting_controller: vars::CONTROLLER_NAME.to_owned(),
    reporting_node: state.config.hostname.clone(),
    kind: EventKind::Normal,
    action: NativeEventAction::Destroy.to_string(),
    related: Some(EventActor {
      key: Some(kind_key),
      kind,
      attributes: None,
    }),
    reason: "state_sync".to_owned(),
    note: Some(format!("Process {name}")),
    metadata: None,
    actor: Some(EventActor {
      key: Some(name.clone()),
      kind: EventActorKind::Process,
      attributes: Some(
        serde_json::to_value(attributes)
          .map_err(|err| err.map_err_context(|| "Event attributes"))?,
      ),
    }),
  };
  match action {
    "destroy" => {
      state.spawn_emit_event(event);
      let _ = ProcessDb::del_by_pk(&id, &state.pool).await;
      return Ok(());
    }
    "create" => {
      event.action = NativeEventAction::Create.to_string();
      state.spawn_emit_event(event);
      return Ok(());
    }
    _ => {
      event.action = action.to_owned();
    }
  }
  state.spawn_emit_event(event);
  let instance = state
    .docker_api
    .inspect_container(&id, None::<InspectContainerOptions>)
    .await
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  let data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  let new_instance = ProcessUpdateDb {
    updated_at: Some(chrono::Utc::now().naive_utc()),
    data: Some(data),
    ..Default::default()
  };
  ProcessDb::update_pk(&id, new_instance, &state.pool).await?;
  Ok(())
}

/// Create a new thread with his own loop to analyze events from docker
pub fn analyze(state: &SystemState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      loop {
        let mut streams =
          state.docker_api.events(None::<EventsOptions<String>>);
        log::info!("event::analyze_docker: stream connected");
        while let Some(event) = streams.next().await {
          match event {
            Ok(event) => {
              if let Err(err) = exec_docker(&event, &state).await {
                log::warn!("event::analyze_docker: {err}")
              }
            }
            Err(err) => {
              log::warn!("event::analyze_docker: {err}");
            }
          }
        }
        log::warn!("event::analyze_docker: disconnected trying to reconnect");
        ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    });
  });
}
