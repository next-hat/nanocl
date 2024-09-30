use futures_util::StreamExt;
use ntex::rt;

use nanocl_error::io::{FromIo, IoResult};

use bollard_next::{
  container::InspectContainerOptions,
  service::{EventMessage, EventMessageTypeEnum},
  system::EventsOptions,
};
use nanocl_stubs::system::{
  EventActor, EventActorKind, EventKind, EventPartial, NativeEventAction,
  ObjPsStatusKind,
};

use crate::{
  models::{
    CargoDb, ObjPsStatusDb, ProcessDb, ProcessUpdateDb, SystemState, VmDb,
  },
  repositories::generic::*,
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
  if !attributes.contains_key("io.nanocl") {
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
    reporting_node: state.inner.config.hostname.clone(),
    kind: EventKind::Normal,
    action: NativeEventAction::Destroy.to_string(),
    related: Some(EventActor {
      key: Some(kind_key.clone()),
      kind: kind.clone(),
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
    "start" => {
      let actual_status =
        ObjPsStatusDb::read_by_pk(&kind_key, &state.inner.pool).await?;
      match (&kind, &actual_status.actual) {
        (EventActorKind::Cargo, status)
          if status != &ObjPsStatusKind::Start.to_string() =>
        {
          ObjPsStatusDb::update_actual_status(
            &kind_key,
            &ObjPsStatusKind::Start,
            &state.inner.pool,
          )
          .await?;
        }
        (EventActorKind::Vm, status)
          if status != &ObjPsStatusKind::Start.to_string() =>
        {
          ObjPsStatusDb::update_actual_status(
            &kind_key,
            &ObjPsStatusKind::Start,
            &state.inner.pool,
          )
          .await?;
        }
        _ => {}
      }
    }
    "die" => {
      if !name.starts_with("tmp-") && !name.starts_with("init-") {
        let actual_status =
          ObjPsStatusDb::read_by_pk(&kind_key, &state.inner.pool).await?;
        log::debug!("Event status wanted {}", actual_status.wanted);
        match (&kind, &actual_status.wanted) {
          (EventActorKind::Cargo, status)
            if status != &ObjPsStatusKind::Stop.to_string() =>
          {
            log::debug!("Set cargo status to fail");
            ObjPsStatusDb::update_actual_status(
              &kind_key,
              &ObjPsStatusKind::Fail,
              &state.inner.pool,
            )
            .await?;
            let cargo =
              CargoDb::transform_read_by_pk(&kind_key, &state.inner.pool)
                .await?;
            state.emit_error_native_action(
              &cargo,
              NativeEventAction::Fail,
              Some(format!("Process {name}")),
            );
          }
          (EventActorKind::Vm, status)
            if status != &ObjPsStatusKind::Stop.to_string() =>
          {
            ObjPsStatusDb::update_actual_status(
              &kind_key,
              &ObjPsStatusKind::Fail,
              &state.inner.pool,
            )
            .await?;
            let vm =
              VmDb::transform_read_by_pk(&kind_key, &state.inner.pool).await?;
            state.emit_error_native_action(
              &vm,
              NativeEventAction::Fail,
              Some(format!("Process {name}")),
            );
          }
          _ => {}
        }
      }
      action.clone_into(&mut event.action);
    }
    "destroy" => {
      state.spawn_emit_event(event);
      let _ = ProcessDb::del_by_pk(&id, &state.inner.pool).await;
      return Ok(());
    }
    "create" => {
      event.action = NativeEventAction::Create.to_string();
      state.spawn_emit_event(event);
      return Ok(());
    }
    _ => {
      action.clone_into(&mut event.action);
    }
  }
  state.spawn_emit_event(event);
  let instance = state
    .inner
    .docker_api
    .inspect_container(&id, None::<InspectContainerOptions>)
    .await
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  let data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  let new_instance = ProcessUpdateDb {
    updated_at: Some(chrono::Utc::now().naive_utc()),
    data: Some(data),
    name: Some(name),
    ..Default::default()
  };
  ProcessDb::update_pk(&id, new_instance, &state.inner.pool).await?;
  Ok(())
}

/// Create a new thread with his own loop to analyze events from docker
pub fn analyze(state: &SystemState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      loop {
        let mut streams =
          state.inner.docker_api.events(None::<EventsOptions<String>>);
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
