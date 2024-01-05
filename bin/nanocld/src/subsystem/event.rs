use std::str::FromStr;

use ntex::rt;
use futures_util::StreamExt;

use nanocl_error::io::IoResult;

use nanocl_stubs::system::{Event, EventActorKind, NativeEventAction};

use crate::{
  utils,
  repositories::generic::*,
  models::{
    SystemState, JobDb, ProcessDb, SystemEventReceiver, SystemEventKind,
  },
};

/// Remove a job after when finished and ttl is set
async fn job_ttl(e: Event, state: &SystemState) -> IoResult<()> {
  let Some(actor) = e.actor else {
    return Ok(());
  };
  if actor.kind != EventActorKind::Process {
    return Ok(());
  }
  let attributes = actor.attributes.unwrap_or_default();
  let job_id = match attributes.get("io.nanocl.j") {
    None => return Ok(()),
    Some(job_id) => job_id.as_str().unwrap_or_default(),
  };
  log::debug!("event::job_ttl: {job_id}");
  let action = NativeEventAction::from_str(e.action.as_str())?;
  match &action {
    NativeEventAction::Create
    | NativeEventAction::Start
    | NativeEventAction::Delete => return Ok(()),
    _ => {}
  }
  let job = JobDb::read_by_pk(job_id, &state.pool)
    .await?
    .try_to_spec()?;
  let ttl = match job.ttl {
    None => return Ok(()),
    Some(ttl) => ttl,
  };
  let instances = ProcessDb::read_by_kind_key(&job.name, &state.pool).await?;
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
async fn exec_event(e: Event, state: &SystemState) -> IoResult<()> {
  job_ttl(e, state).await?;
  Ok(())
}

/// Read events from the event stream
async fn read_events(stream: &mut SystemEventReceiver, state: &SystemState) {
  while let Some(e) = stream.next().await {
    if let SystemEventKind::Emit(e) = e {
      if let Err(err) = exec_event(e, state).await {
        log::warn!("event::read_events: {err}");
      }
    }
  }
}

/// Spawn a tread to analize events from the event stream in his own loop
pub(crate) fn analize(state: &SystemState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(|| {
    rt::spawn(async move {
      loop {
        let mut stream = match state.subscribe().await {
          Ok(stream) => stream,
          Err(err) => {
            log::error!("event::analize: {err}");
            continue;
          }
        };
        log::info!("event::analize: stream connected");
        read_events(&mut stream, &state).await;
        ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    });
  });
}
