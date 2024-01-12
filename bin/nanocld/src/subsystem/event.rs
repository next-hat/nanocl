use std::{str::FromStr, sync::Arc};

use bollard_next::container::{StartContainerOptions, RemoveContainerOptions};
use ntex::rt;
use futures_util::StreamExt;

use nanocl_error::io::IoResult;

use nanocl_stubs::system::{
  Event, EventActorKind, NativeEventAction, ObjPsStatusKind,
};

use crate::{
  utils,
  objects::generic::*,
  repositories::generic::*,
  models::{
    SystemState, JobDb, ProcessDb, SystemEventReceiver, SystemEventKind,
    CargoDb, ObjPsStatusUpdate, ObjPsStatusDb,
  },
};

/// Remove a job after when finished and ttl is set
async fn job_ttl(e: &Event, state: &SystemState) -> IoResult<()> {
  let Some(ref actor) = e.actor else {
    return Ok(());
  };
  if actor.kind != EventActorKind::Process {
    return Ok(());
  }
  let attributes = actor.attributes.clone().unwrap_or_default();
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
      let _ = JobDb::del_obj_by_pk(&job.name, &(), &state).await;
    });
  }
  Ok(())
}

async fn start(e: &Event, state: &SystemState) -> IoResult<()> {
  let action = NativeEventAction::from_str(e.action.as_str())?;
  // If it's not a start action, we don't care
  if action != NativeEventAction::Starting {
    return Ok(());
  }
  // If there is no actor, we don't care
  let Some(ref actor) = e.actor else {
    return Ok(());
  };
  let key = actor.key.clone().unwrap_or_default();
  match actor.kind {
    EventActorKind::Cargo => {
      log::debug!("handling start event for cargo {key}");
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      let mut processes =
        ProcessDb::read_by_kind_key(&key, &state.pool).await?;
      if processes.is_empty() {
        processes = utils::cargo::create_instances(&cargo, 1, state).await?;
      }
      for process in processes {
        let _ = state
          .docker_api
          .start_container(&process.key, None::<StartContainerOptions<String>>)
          .await;
      }
      let cur_status = ObjPsStatusDb::read_by_pk(&key, &state.pool).await?;
      let new_status = ObjPsStatusUpdate {
        wanted: Some(ObjPsStatusKind::Running.to_string()),
        prev_wanted: Some(cur_status.wanted),
        actual: Some(ObjPsStatusKind::Running.to_string()),
        prev_actual: Some(cur_status.actual),
      };
      ObjPsStatusDb::update_pk(&key, new_status, &state.pool).await?;
      state.emit_normal_native_action(&cargo, NativeEventAction::Start);
    }
    EventActorKind::Vm => {}
    EventActorKind::Job => {}
    _ => {}
  }
  Ok(())
}

async fn delete(e: &Event, state: &SystemState) -> IoResult<()> {
  let action = NativeEventAction::from_str(e.action.as_str())?;
  // If it's not a start action, we don't care
  if action != NativeEventAction::Deleting {
    return Ok(());
  }
  // If there is no actor, we don't care
  let Some(ref actor) = e.actor else {
    return Ok(());
  };
  let key = actor.key.clone().unwrap_or_default();
  let processes = ProcessDb::read_by_kind_key(&key, &state.pool).await?;
  for process in processes {
    let _ = state
      .docker_api
      .remove_container(
        &process.key,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
      )
      .await;
  }
  match actor.kind {
    EventActorKind::Cargo => {
      log::debug!("handling delete event for cargo {key}");
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      CargoDb::clear_by_pk(&key, &state.pool).await?;
      state.emit_normal_native_action(&cargo, NativeEventAction::Delete);
    }
    EventActorKind::Vm => {}
    EventActorKind::Job => {}
    _ => {}
  };
  Ok(())
}

async fn update(e: &Event, state: &SystemState) -> IoResult<()> {
  let action = NativeEventAction::from_str(e.action.as_str())?;
  // If it's not a start action, we don't care
  if action != NativeEventAction::Update {
    return Ok(());
  }
  // If there is no actor, we don't care
  let Some(ref actor) = e.actor else {
    return Ok(());
  };
  let key = actor.key.clone().unwrap_or_default();
  let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
  let processes = ProcessDb::read_by_kind_key(&key, &state.pool).await?;
  // Create instance with the new spec
  let mut error = None;
  let new_instances =
    match utils::cargo::create_instances(&cargo, 1, state).await {
      Err(err) => {
        error = Some(err);
        Vec::default()
      }
      Ok(instances) => instances,
    };
  // start created containers
  match CargoDb::start_process_by_kind_key(&key, state).await {
    Err(err) => {
      log::error!(
        "Unable to start cargo instance {} : {err}",
        cargo.spec.cargo_key
      );
      let state_ptr = state.clone();
      rt::spawn(async move {
        ntex::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = utils::cargo::delete_instances(
          &new_instances
            .iter()
            .map(|i| i.key.clone())
            .collect::<Vec<_>>(),
          &state_ptr,
        )
        .await;
      });
    }
    Ok(_) => {
      // Delete old containers
      utils::cargo::delete_instances(
        &processes.iter().map(|c| c.key.clone()).collect::<Vec<_>>(),
        state,
      )
      .await?;
    }
  }
  match error {
    Some(err) => Err(err.into()),
    None => Ok(()),
  }
}

/// Take action when event is received
async fn exec_event(e: Event, state: &SystemState) -> IoResult<()> {
  log::debug!("exec_event: {} {}", e.kind, e.action);
  job_ttl(&e, state).await?;
  start(&e, state).await?;
  delete(&e, state).await?;
  update(&e, state).await?;
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
pub fn analize(state: &SystemState) {
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
