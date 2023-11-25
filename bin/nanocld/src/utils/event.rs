use ntex::rt;

use nanocl_error::io::FromIo;
use nanocl_error::http::HttpResult;

use nanocl_stubs::system::{Event, EventKind, EventAction};

use crate::{utils, repositories};
use crate::event::Client;
use crate::models::DaemonState;

async fn job_ttl(e: Event, state: &DaemonState) -> HttpResult<()> {
  if e.kind != EventKind::ContainerInstance {
    return Ok(());
  }
  let actor = e.actor.unwrap_or_default();
  let attributes = actor.attributes.unwrap_or_default();
  log::debug!("Job auto remove attributes: {attributes}");
  let job_id = match attributes.get("io.nanocl.j") {
    None => return Ok(()),
    Some(job_id) => job_id.as_str().unwrap_or_default(),
  };
  match &e.action {
    EventAction::Created | EventAction::Started | EventAction::Deleted => {
      return Ok(())
    }
    _ => {}
  }
  let job = repositories::job::find_by_name(job_id, &state.pool).await?;
  let ttl = match job.ttl {
    None => return Ok(()),
    Some(ttl) => ttl,
  };
  let instances = utils::job::inspect_instances(&job.name, state).await?;
  let (_, _, _, running) = utils::job::count_instances(&instances);
  if running == 0 && !instances.is_empty() {
    let state = state.clone();
    rt::spawn(async move {
      log::debug!("Job {} will be deleted in {ttl}s", job.name);
      ntex::time::sleep(std::time::Duration::from_secs(ttl as u64)).await;
      let _ = utils::job::delete_by_name(&job.name, &state).await;
    });
  }
  Ok(())
}

async fn analize_event(e: Event, state: &DaemonState) -> HttpResult<()> {
  job_ttl(e, state).await?;
  Ok(())
}

async fn extract_event(stream: &mut Client) -> HttpResult<Event> {
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
    if let Err(err) = analize_event(e, state).await {
      log::warn!("{err}");
    }
  }
}

pub fn analize_events(state: &DaemonState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(|| {
    rt::spawn(async move {
      loop {
        let mut stream = match state.event_emitter.subscribe().await {
          Ok(stream) => stream,
          Err(err) => {
            log::error!("{err}");
            continue;
          }
        };
        log::debug!("Internal event stream connected");
        read_events(&mut stream, &state).await;
      }
    });
  });
}
