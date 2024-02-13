use std::str::FromStr;

use ntex::rt;

use nanocl_error::io::IoResult;
use nanocl_stubs::system::{Event, EventActor, EventActorKind, NativeEventAction};

use crate::{
  utils,
  tasks::generic::*,
  objects::generic::*,
  repositories::generic::*,
  models::{SystemState, JobDb, ProcessDb, CargoDb, ObjTask},
};

/// Remove a job after when finished and ttl is set
async fn job_ttl(actor: &EventActor, state: &SystemState) -> IoResult<()> {
  let attributes = actor.attributes.clone().unwrap_or_default();
  let job_id = match attributes.get("io.nanocl.j") {
    None => return Ok(()),
    Some(job_id) => job_id.as_str().unwrap_or_default(),
  };
  log::debug!("event::job_ttl: {job_id}");
  let job = JobDb::read_by_pk(job_id, &state.pool)
    .await?
    .try_to_spec()?;
  let instances = ProcessDb::read_by_kind_key(&job.name, &state.pool).await?;
  let (_, instance_failed, _, running) =
    utils::container::count_status(&instances);
  log::debug!(
    "event::job_ttl: {} has {running} running instances",
    job.name
  );
  if running != 0 {
    return Ok(());
  }
  if instance_failed > 0 {
    state.emit_normal_native_action(&job, NativeEventAction::Fail);
  }
  state.emit_normal_native_action(&job, NativeEventAction::Finish);
  let ttl = match job.ttl {
    None => return Ok(()),
    Some(ttl) => ttl,
  };
  let state = state.clone();
  rt::spawn(async move {
    log::debug!("event::job_ttl: {} will be deleted in {ttl}s", job.name);
    ntex::time::sleep(std::time::Duration::from_secs(ttl as u64)).await;
    let _ = JobDb::del_obj_by_pk(&job.name, &(), &state).await;
  });
  Ok(())
}

async fn start(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> IoResult<Option<ObjTaskFuture>> {
  let task = match actor.kind {
    EventActorKind::Job => {
      let task = JobDb::create_start_task(key, state);
      Some(task)
    }
    EventActorKind::Cargo => {
      let task = CargoDb::create_start_task(key, state);
      Some(task)
    }
    EventActorKind::Vm => None,
    _ => None,
  };
  Ok(task)
}

/// Handle delete event for living objects (job, cargo, vm)
/// by checking if the event is `NativeEventAction::Deleting`
/// and pushing into the task manager the task to delete the object
async fn delete(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> IoResult<Option<ObjTaskFuture>> {
  let task = match actor.kind {
    EventActorKind::Cargo => {
      log::debug!("handling delete event for cargo {key}");
      let task = CargoDb::create_delete_task(key, state);
      Some(task)
    }
    EventActorKind::Vm => None,
    EventActorKind::Job => {
      let task = JobDb::create_delete_task(key, state);
      Some(task)
    }
    _ => None,
  };
  Ok(task)
}

async fn update(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> IoResult<Option<ObjTaskFuture>> {
  let task = match actor.kind {
    EventActorKind::Cargo => {
      let task = CargoDb::create_update_task(key, state);
      Some(task)
    }
    _ => None,
  };
  Ok(task)
}

async fn stop(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> IoResult<Option<ObjTaskFuture>> {
  let task = match actor.kind {
    EventActorKind::Cargo => {
      let task = CargoDb::create_stop_task(key, state);
      Some(task)
    }
    _ => None,
  };
  Ok(task)
}

/// Take action when event is received
/// and push the action into the task manager
/// The task manager will execute the action in background
/// eg: starting, deleting, updating a living object
pub async fn exec_event(e: &Event, state: &SystemState) -> IoResult<()> {
  let Some(ref actor) = e.actor else {
    return Ok(());
  };
  let key = actor.key.clone().unwrap_or_default();
  log::info!(
    "exec_event: {} {} {}",
    e.kind,
    e.action,
    actor.key.clone().unwrap_or_default()
  );
  // Specific key of the task for this object
  // If a task is already running for this object, we wait for it to finish
  // This is to avoid data races conditions when manipulating an object
  let task_key = format!("{}@{key}", &actor.kind);
  state.task_manager.wait_task(&task_key).await;
  let action = NativeEventAction::from_str(e.action.as_str())?;
  let task: Option<ObjTaskFuture> = match action {
    NativeEventAction::Create => None,
    NativeEventAction::Starting => start(&key, actor, state).await?,
    NativeEventAction::Stopping => stop(&key, actor, state).await?,
    NativeEventAction::Updating => update(&key, actor, state).await?,
    NativeEventAction::Destroying => delete(&key, actor, state).await?,
    NativeEventAction::Destroy => None,
    NativeEventAction::Fail => None,
    NativeEventAction::Finish => None,
    NativeEventAction::Start => None,
    NativeEventAction::Stop => None,
    _ => {
      job_ttl(actor, state).await?;
      None
    }
  };
  let Some(task) = task else { return Ok(()) };
  // push the task into the task manager
  state.task_manager.add_task(&task_key, action, task).await;
  Ok(())
}
