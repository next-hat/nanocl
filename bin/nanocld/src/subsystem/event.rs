use std::str::FromStr;

use ntex::rt;
use futures_util::StreamExt;
use bollard_next::container::{
  StartContainerOptions, RemoveContainerOptions, WaitContainerOptions,
};

use nanocl_error::{
  io::{IoResult, IoError},
  http::HttpError,
};
use nanocl_stubs::{
  system::{Event, EventActorKind, NativeEventAction, ObjPsStatusKind},
  process::ProcessKind,
};

use crate::{
  utils,
  objects::generic::*,
  repositories::generic::*,
  models::{
    SystemState, JobDb, ProcessDb, CargoDb, ObjPsStatusUpdate, ObjPsStatusDb,
    ObjTask,
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
  let action = NativeEventAction::from_str(e.action.as_str())?;
  match &action {
    NativeEventAction::Create
    | NativeEventAction::Start
    | NativeEventAction::Delete => return Ok(()),
    _ => {}
  }
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
  let (_, _, _, running) = utils::process::count_status(&instances);
  log::debug!(
    "event::job_ttl: {} has {running} running instances",
    job.name
  );
  if running != 0 {
    return Ok(());
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
      let task_key = format!("{}@{key}", actor.kind);
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      let state_ptr = state.clone();
      let curr_task = state.task_manager.get_task(&task_key).await;
      if curr_task.is_some() {
        state.task_manager.remove_task(&task_key).await?;
      }
      let task = ObjTask::new(action, async move {
        let mut processes =
          ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state_ptr.pool)
            .await?;
        if processes.is_empty() {
          processes =
            utils::cargo::create_instances(&cargo, 1, &state_ptr).await?;
        }
        for process in processes {
          let _ = state_ptr
            .docker_api
            .start_container(
              &process.key,
              None::<StartContainerOptions<String>>,
            )
            .await;
        }
        let cur_status =
          ObjPsStatusDb::read_by_pk(&cargo.spec.cargo_key, &state_ptr.pool)
            .await?;
        let new_status = ObjPsStatusUpdate {
          wanted: Some(ObjPsStatusKind::Running.to_string()),
          prev_wanted: Some(cur_status.wanted),
          actual: Some(ObjPsStatusKind::Running.to_string()),
          prev_actual: Some(cur_status.actual),
        };
        ObjPsStatusDb::update_pk(
          &cargo.spec.cargo_key,
          new_status,
          &state_ptr.pool,
        )
        .await?;
        state_ptr.emit_normal_native_action(&cargo, NativeEventAction::Start);
        Ok::<_, IoError>(())
      });
      state.task_manager.add_task(&task_key, task).await?;
    }
    EventActorKind::Vm => {}
    EventActorKind::Job => {
      let task_key = format!("{}@{key}", actor.kind);
      let job = JobDb::read_by_pk(&key, &state.pool).await?.try_to_spec()?;
      let state_ptr = state.clone();
      let curr_task = state.task_manager.get_task(&task_key).await;
      if curr_task.is_some() {
        state.task_manager.remove_task(&task_key).await?;
      }
      let task = ObjTask::new(action, async move {
        for mut container in job.containers {
          let job_name = job.name.clone();
          let mut labels = container.labels.clone().unwrap_or_default();
          labels.insert("io.nanocl.j".to_owned(), job_name.clone());
          container.labels = Some(labels);
          let short_id = utils::key::generate_short_id(6);
          let name = format!("{job_name}-{short_id}.j");
          let process = utils::container::create_process(
            &ProcessKind::Job,
            &name,
            &job_name,
            container,
            &state_ptr,
          )
          .await?;
          // When we run a sequential order we wait for the container to finish to start the next one.
          let mut stream = state_ptr.docker_api.wait_container(
            &process.key,
            Some(WaitContainerOptions {
              condition: "not-running",
            }),
          );
          let _ = state_ptr
            .docker_api
            .start_container(
              &process.key,
              None::<StartContainerOptions<String>>,
            )
            .await;
          while let Some(stream) = stream.next().await {
            let result = stream.map_err(HttpError::internal_server_error)?;
            if result.status_code == 0 {
              break;
            }
          }
        }
        Ok::<_, IoError>(())
      });
      state.task_manager.add_task(&task_key, task).await?;
    }
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
  match actor.kind {
    EventActorKind::Cargo => {
      log::debug!("handling delete event for cargo {key}");
      let task_key = format!("{}@{key}", &actor.kind);
      let curr_task = state.task_manager.get_task(&task_key).await;
      if curr_task.is_some() {
        state.task_manager.remove_task(&task_key).await?;
      }
      let state_ptr = state.clone();
      let task = ObjTask::new(action, async move {
        let processes =
          ProcessDb::read_by_kind_key(&key, &state_ptr.pool).await?;
        for process in processes {
          let _ = state_ptr
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
        let cargo =
          CargoDb::transform_read_by_pk(&key, &state_ptr.pool).await?;
        CargoDb::clear_by_pk(&key, &state_ptr.pool).await?;
        state_ptr.emit_normal_native_action(&cargo, NativeEventAction::Delete);
        Ok::<_, IoError>(())
      });
      state.task_manager.add_task(&task_key, task).await?;
    }
    EventActorKind::Vm => {}
    EventActorKind::Job => {
      let job = JobDb::read_by_pk(&key, &state.pool).await?.try_to_spec()?;
      let task_key = format!("{}@{key}", &actor.kind);
      let curr_task = state.task_manager.get_task(&task_key).await;
      if curr_task.is_some() {
        state.task_manager.remove_task(&task_key).await?;
      }
      let state_ptr = state.clone();
      let task = ObjTask::new(action, async move {
        let processes =
          ProcessDb::read_by_kind_key(&key, &state_ptr.pool).await?;
        for process in processes {
          let _ = state_ptr
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
        JobDb::clear(&job.name, &state_ptr.pool).await?;
        if job.schedule.is_some() {
          utils::job::remove_cron_rule(&job, &state_ptr).await?;
        }
        state_ptr.emit_normal_native_action(&job, NativeEventAction::Delete);
        Ok::<_, IoError>(())
      });
      state.task_manager.add_task(&task_key, task).await?;
    }
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
  match actor.kind {
    EventActorKind::Cargo => {
      let task_key = format!("{}@{key}", &actor.kind);
      let curr_task = state.task_manager.get_task(&task_key).await;
      if curr_task.is_some() {
        state.task_manager.remove_task(&task_key).await?;
      }
      let state_ptr = state.clone();
      let task = ObjTask::new(action, async move {
        let cargo =
          CargoDb::transform_read_by_pk(&key, &state_ptr.pool).await?;
        let processes =
          ProcessDb::read_by_kind_key(&key, &state_ptr.pool).await?;
        // Create instance with the new spec
        let new_instances =
          match utils::cargo::create_instances(&cargo, 1, &state_ptr).await {
            Err(err) => {
              log::warn!(
                "Unable to create cargo instance {} : {err}",
                cargo.spec.cargo_key
              );
              Vec::default()
            }
            Ok(instances) => instances,
          };
        // start created containers
        match CargoDb::start_process_by_kind_key(&key, &state_ptr).await {
          Err(err) => {
            log::error!(
              "Unable to start cargo instance {} : {err}",
              cargo.spec.cargo_key
            );
            let state_ptr_ptr = state_ptr.clone();
            rt::spawn(async move {
              ntex::time::sleep(std::time::Duration::from_secs(2)).await;
              let _ = utils::cargo::delete_instances(
                &new_instances
                  .iter()
                  .map(|i| i.key.clone())
                  .collect::<Vec<_>>(),
                &state_ptr_ptr,
              )
              .await;
            });
          }
          Ok(_) => {
            // Delete old containers
            utils::cargo::delete_instances(
              &processes.iter().map(|c| c.key.clone()).collect::<Vec<_>>(),
              &state_ptr,
            )
            .await?;
          }
        }
        Ok::<_, IoError>(())
      });
      state.task_manager.add_task(&task_key, task).await?;
    }
    EventActorKind::Vm => {}
    EventActorKind::Job => {}
    _ => {}
  }
  Ok(())
}

/// Take action when event is received
pub async fn exec_event(ev: &Event, state: &SystemState) -> IoResult<()> {
  log::debug!("exec_event: {} {}", ev.kind, ev.action);
  start(ev, state).await?;
  delete(ev, state).await?;
  update(ev, state).await?;
  job_ttl(ev, state).await?;
  Ok(())
}
