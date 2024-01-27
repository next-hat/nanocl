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
  repositories::generic::*,
  models::{SystemState, ProcessUpdateDb, ProcessDb},
  vars,
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
  let action = event.action.clone().unwrap_or_default();
  let id = actor.id.unwrap_or_default();
  let action = action.as_str();
  let mut event = EventPartial {
    reporting_controller: vars::CONTROLLER_NAME.to_owned(),
    reporting_node: state.config.hostname.clone(),
    kind: EventKind::Normal,
    action: NativeEventAction::Delete.to_string(),
    related: None,
    reason: "state_sync".to_owned(),
    note: None,
    metadata: None,
    actor: Some(EventActor {
      key: Some(id.clone()),
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
