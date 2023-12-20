use ntex::rt;
use futures_util::StreamExt;

use nanocl_error::io::{FromIo, IoResult};

use bollard_next::{
  system::EventsOptions,
  container::InspectContainerOptions,
  service::{EventMessageTypeEnum, EventMessage},
};
use nanocl_stubs::system::{Event, EventKind, EventAction, EventActor};

use crate::{
  utils,
  event_emitter::Client,
  repositories::generic::*,
  models::{DaemonState, JobDb, FromSpec, ProcessUpdateDb, ProcessDb},
};

/// Remove a job after when finished and ttl is set
async fn job_ttl(e: Event, state: &DaemonState) -> IoResult<()> {
  if e.kind != EventKind::Process {
    return Ok(());
  }
  let actor = e.actor.unwrap_or_default();
  let attributes = actor.attributes.unwrap_or_default();
  let job_id = match attributes.get("io.nanocl.j") {
    None => return Ok(()),
    Some(job_id) => job_id.as_str().unwrap_or_default(),
  };
  log::debug!("event::job_ttl: {job_id}");
  match &e.action {
    EventAction::Created | EventAction::Started | EventAction::Deleted => {
      return Ok(())
    }
    _ => {}
  }
  let job = JobDb::read_by_pk(job_id, &state.pool)
    .await??
    .try_to_spec()?;
  let ttl = match job.ttl {
    None => return Ok(()),
    Some(ttl) => ttl,
  };
  let instances = ProcessDb::find_by_kind_key(&job.name, &state.pool).await?;
  let (_, _, _, running) = utils::process::count_status(&instances);
  log::debug!(
    "event::job_ttl: {} has {running} running instances",
    job.name
  );
  if running == 0 {
    let state = state.clone();
    rt::spawn(async move {
      log::debug!("event::job_ttl: {} will be deleted in {ttl}s", job.name);
      ntex::time::sleep(std::time::Duration::from_secs(ttl as u64)).await;
      let _ = utils::job::delete_by_name(&job.name, &state).await;
    });
  }
  Ok(())
}

/// Take action when event is received
async fn exec_event(e: Event, state: &DaemonState) -> IoResult<()> {
  job_ttl(e, state).await?;
  Ok(())
}

/// Extract an event from the stream
async fn extract_event(stream: &mut Client) -> IoResult<Event> {
  let mut payload: Vec<u8> = Vec::new();
  while let Some(bytes) = stream.recv().await {
    payload.extend(&bytes);
    if bytes.last() == Some(&b'\n') {
      break;
    }
  }
  let e = serde_json::from_slice::<Event>(&payload)
    .map_err(|err| err.map_err_context(|| "Event deserialization error"))?;
  Ok(e)
}

/// Read events from the event stream
async fn read_events(stream: &mut Client, state: &DaemonState) {
  loop {
    let e = extract_event(stream).await;
    let e = match e {
      Err(err) => {
        log::error!("{err}");
        continue;
      }
      Ok(e) => e,
    };
    if let Err(err) = exec_event(e, state).await {
      log::warn!("event::read_events: {err}");
    }
  }
}

/// Spawn a tread to analize events from the event stream in his own loop
pub(crate) fn analize(state: &DaemonState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(|| {
    rt::spawn(async move {
      loop {
        let mut stream = match state.event_emitter.subscribe().await {
          Ok(stream) => stream,
          Err(err) => {
            log::error!("event::analize: {err}");
            continue;
          }
        };
        log::info!("event::analize: stream connected");
        read_events(&mut stream, &state).await;
      }
    });
  });
}

/// Take actions when a docker event is received
async fn exec_docker(
  event: &EventMessage,
  state: &DaemonState,
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
  log::debug!("event::exec_docker: {action}");
  let action = action.as_str();
  let mut event = Event {
    kind: EventKind::Process,
    action: EventAction::Deleted,
    actor: Some(EventActor {
      key: Some(id.clone()),
      attributes: Some(
        serde_json::to_value(attributes)
          .map_err(|err| err.map_err_context(|| "Event attributes"))?,
      ),
    }),
  };
  match action {
    "destroy" => {
      state.event_emitter.spawn_emit_event(event);
      let _ = ProcessDb::del_by_pk(&id, &state.pool).await?;
      return Ok(());
    }
    "create" => {
      event.action = EventAction::Created;
      state.event_emitter.spawn_emit_event(event);
      return Ok(());
    }
    "start" => {
      event.action = EventAction::Started;
    }
    "stop" => {
      event.action = EventAction::Stopped;
    }
    "restart" => {
      event.action = EventAction::Restart;
    }
    _ => {
      event.action = EventAction::Patched;
    }
  }
  state.event_emitter.spawn_emit_event(event);
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
  ProcessDb::update_pk(&id, new_instance, &state.pool).await??;
  Ok(())
}

/// Create a new thread with his own loop to analize events from docker
pub(crate) fn analize_docker(state: &DaemonState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      loop {
        let mut streams =
          state.docker_api.events(None::<EventsOptions<String>>);
        log::info!("event::analize_docker: stream connected");
        while let Some(event) = streams.next().await {
          match event {
            Ok(event) => {
              if let Err(err) = exec_docker(&event, &state).await {
                log::warn!("event::analize_docker: {err}")
              }
            }
            Err(err) => {
              log::warn!("event::analize_docker: {err}");
            }
          }
        }
        log::warn!("event::analize_docker: disconnected trying to reconnect");
        ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    });
  });
}
