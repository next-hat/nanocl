use std::str::FromStr;

use ntex::rt;

use nanocl_error::io::IoResult;
use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  system::{
    Event, EventActor, EventActorKind, EventKind, NativeEventAction,
    ObjPsStatusKind,
  },
};

use crate::{
  models::{CargoDb, JobDb, ObjPsStatusDb, ProcessDb, SystemState, VmDb},
  objects::generic::*,
  repositories::generic::*,
  tasks::generic::*,
  utils,
};

/// Remove a job after when finished and ttl is set
async fn job_ttl(actor: &EventActor, state: &SystemState) -> IoResult<()> {
  let attributes = actor.attributes.clone().unwrap_or_default();
  let job_id = match attributes.get("io.nanocl.j") {
    None => return Ok(()),
    Some(job_id) => job_id.as_str().unwrap_or_default(),
  };
  log::debug!("event::job_ttl: {job_id}");
  let job = JobDb::transform_read_by_pk(job_id, &state.inner.pool).await?;
  match job.status.actual {
    ObjPsStatusKind::Finish | ObjPsStatusKind::Fail => {
      log::debug!("event::job_ttl: {job_id} is already done");
      return Ok(());
    }
    _ => {}
  }
  let instances =
    ProcessDb::read_by_kind_key(&job.name, &state.inner.pool).await?;
  let (_, instance_failed, _, running) =
    utils::container::generic::count_status(&instances);
  log::debug!(
    "event::job_ttl: {} has {running} running instances",
    job.name
  );
  if running != 0 {
    return Ok(());
  }
  log::debug!("instance_failed: {instance_failed}");
  if instance_failed > 0 {
    ObjPsStatusDb::update_actual_status(
      &job.name,
      &ObjPsStatusKind::Fail,
      &state.inner.pool,
    )
    .await?;
    state
      .emit_normal_native_action_sync(&job, NativeEventAction::Fail)
      .await;
  } else {
    ObjPsStatusDb::update_actual_status(
      &job.name,
      &ObjPsStatusKind::Finish,
      &state.inner.pool,
    )
    .await?;
    state
      .emit_normal_native_action_sync(&job, NativeEventAction::Finish)
      .await;
  }
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

fn starting(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> Option<ObjTaskFuture> {
  match actor.kind {
    EventActorKind::Job => {
      let task = JobDb::create_start_task(key, state);
      Some(task)
    }
    EventActorKind::Cargo => {
      let task = CargoDb::create_start_task(key, state);
      Some(task)
    }
    EventActorKind::Vm => {
      let task = VmDb::create_start_task(key, state);
      Some(task)
    }
    _ => None,
  }
}

/// Handle delete event for living objects (job, cargo, vm)
/// by checking if the event is `NativeEventAction::Deleting`
/// and pushing into the task manager the task to delete the object
fn destroying(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> Option<ObjTaskFuture> {
  match actor.kind {
    EventActorKind::Cargo => {
      log::debug!("handling delete event for cargo {key}");
      let task = CargoDb::create_delete_task(key, state);
      Some(task)
    }
    EventActorKind::Vm => {
      let task = VmDb::create_delete_task(key, state);
      Some(task)
    }
    EventActorKind::Job => {
      let task = JobDb::create_delete_task(key, state);
      Some(task)
    }
    _ => None,
  }
}

fn updating(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> Option<ObjTaskFuture> {
  match actor.kind {
    EventActorKind::Cargo => {
      let task = CargoDb::create_update_task(key, state);
      Some(task)
    }
    EventActorKind::Vm => {
      let task = VmDb::create_update_task(key, state);
      Some(task)
    }
    _ => None,
  }
}

async fn update(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> Option<ObjTaskFuture> {
  match actor.kind {
    // If a secret is updated we check for the cargoes using it and fire an update for them
    EventActorKind::Secret => {
      log::debug!("handling update event for secret {key}");
      let filter = GenericFilter::new().r#where(
        "data",
        GenericClause::Contains(serde_json::json!({
          "Secrets": [
            key
          ]
        })),
      );
      let cargoes = CargoDb::transform_read_by(&filter, &state.inner.pool)
        .await
        .unwrap();
      log::debug!("found {} cargoes using secret {key}", cargoes.len());
      for cargo in &cargoes {
        ObjPsStatusDb::update_actual_status(
          &cargo.spec.cargo_key,
          &ObjPsStatusKind::Updating,
          &state.inner.pool,
        )
        .await
        .ok();
        state
          .emit_normal_native_action_sync(cargo, NativeEventAction::Updating)
          .await;
      }
      None
    }
    _ => None,
  }
}

fn stopping(
  key: &str,
  actor: &EventActor,
  state: &SystemState,
) -> Option<ObjTaskFuture> {
  match actor.kind {
    EventActorKind::Cargo => {
      let task = CargoDb::create_stop_task(key, state);
      Some(task)
    }
    EventActorKind::Vm => {
      let task = VmDb::create_stop_task(key, state);
      Some(task)
    }
    EventActorKind::Job => {
      let task = JobDb::create_stop_task(key, state);
      Some(task)
    }
    _ => None,
  }
}

/// Take action when event is received
/// and push the action into the task manager
/// The task manager will execute the action in background
/// eg: starting, deleting, updating a living object
pub async fn exec_event(e: &Event, state: &SystemState) -> IoResult<()> {
  match e.kind {
    EventKind::Error | EventKind::Warning => return Ok(()),
    _ => {}
  }
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
  let action = NativeEventAction::from_str(e.action.as_str())?;
  match (&actor.kind, &action) {
    (EventActorKind::Cargo | EventActorKind::Vm, _) => {
      state.inner.task_manager.wait_task(&task_key).await;
    }
    (EventActorKind::Job, NativeEventAction::Destroying) => {
      log::debug!("Removing task for job {key}");
      state.inner.task_manager.remove_task(&task_key).await;
    }
    _ => {}
  }
  let task: Option<ObjTaskFuture> = match action {
    NativeEventAction::Starting => starting(&key, actor, state),
    NativeEventAction::Stopping => stopping(&key, actor, state),
    NativeEventAction::Updating => updating(&key, actor, state),
    NativeEventAction::Update => update(&key, actor, state).await,
    NativeEventAction::Destroying => destroying(&key, actor, state),
    NativeEventAction::Die => {
      job_ttl(actor, state).await?;
      None
    }
    _ => None,
  };
  let Some(task) = task else { return Ok(()) };
  // push the task into the task manager
  let state_ptr = state.clone();
  let actor = actor.clone();
  state
    .inner
    .task_manager
    .add_task(&task_key, action.clone(), task, |err| async move {
      log::error!(
        "exec_event add_task: error {action} {} {:#?} {err}",
        actor.kind,
        actor.key
      );
      let action = match action {
        NativeEventAction::Starting => NativeEventAction::Start,
        NativeEventAction::Stopping => NativeEventAction::Stop,
        NativeEventAction::Updating => NativeEventAction::Update,
        NativeEventAction::Destroying => NativeEventAction::Destroy,
        _ => return Ok(()),
      };
      let state = state_ptr.clone();
      let key = actor.key.clone().unwrap_or_default();
      ObjPsStatusDb::update_actual_status(
        &key,
        &ObjPsStatusKind::Fail,
        &state.inner.pool,
      )
      .await?;
      state_ptr.emit_error_native_action(&actor, action, Some(err.to_string()));
      Ok(())
    })
    .await;
  Ok(())
}
