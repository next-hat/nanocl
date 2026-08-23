use bollard_next::container::WaitContainerOptions;
use futures::StreamExt;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{cargo::Cargo, node::CargoReplicaTask, process::Process};

use crate::models::{
  CargoReplicaProcessRole, CompileDeclaredOpts, EnsureExistingSlotOpts,
  SystemState,
};

use super::{
  RuntimeSlot, compile_declared, create_process, ensure_existing_slot,
  ensure_mapping, inspect_process, start_process,
};

pub(super) async fn reconcile_containers(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  attempt_id: &str,
  gateway: &str,
  slots: &mut Vec<RuntimeSlot>,
  state: &SystemState,
) -> IoResult<()> {
  for declared in &cargo.spec.init_containers {
    let config = compile_declared(CompileDeclaredOpts {
      cargo,
      task,
      declared,
      role: CargoReplicaProcessRole::Init,
      sandbox_id: None,
      attempt_id,
      gateway,
      state,
    })
    .await?;
    let mapping = ensure_mapping(
      task,
      CargoReplicaProcessRole::Init,
      &declared.name,
      true,
      state,
    )
    .await?;
    let slot = ensure_existing_slot(EnsureExistingSlotOpts {
      cargo,
      task,
      mapping,
      role: CargoReplicaProcessRole::Init,
      container_name: &declared.name,
      essential: true,
      config: &config,
      state,
    })
    .await?;
    wait_init(&slot.process, state).await?;
    slots.push(slot);
  }
  Ok(())
}

pub(super) async fn create_candidate_containers(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  attempt_id: &str,
  gateway: &str,
  slots: &mut Vec<RuntimeSlot>,
  state: &SystemState,
) -> IoResult<()> {
  for declared in &cargo.spec.init_containers {
    let config = compile_declared(CompileDeclaredOpts {
      cargo,
      task,
      declared,
      role: CargoReplicaProcessRole::Init,
      sandbox_id: None,
      attempt_id,
      gateway,
      state,
    })
    .await?;
    let mapping = ensure_mapping(
      task,
      CargoReplicaProcessRole::Init,
      &declared.name,
      true,
      state,
    )
    .await?;
    let process = create_process(
      cargo,
      task,
      CargoReplicaProcessRole::Init,
      &declared.name,
      &config,
      true,
      state,
    )
    .await?;
    let slot = RuntimeSlot {
      mapping,
      process,
      role: CargoReplicaProcessRole::Init,
      container_name: declared.name.clone(),
      essential: true,
    };
    slots.push(slot);
    wait_init(
      &slots
        .last()
        .expect("an init candidate was just inserted")
        .process,
      state,
    )
    .await?;
  }
  Ok(())
}

async fn wait_init(process: &Process, state: &SystemState) -> IoResult<()> {
  let inspected = inspect_process(&process.key, state).await?;
  let container_state = inspected.state.as_ref();
  let running = container_state
    .and_then(|state| state.running)
    .unwrap_or_default();
  let status = container_state
    .and_then(|state| state.status.as_ref())
    .map(ToString::to_string);
  let exit_code = container_state.and_then(|state| state.exit_code);
  match init_container_disposition(running, status.as_deref(), exit_code) {
    InitContainerDisposition::Complete => return Ok(()),
    InitContainerDisposition::Start => start_process(process, state).await?,
    InitContainerDisposition::Wait => {}
  }
  let mut stream = state.inner.docker_api.wait_container(
    &process.key,
    Some(WaitContainerOptions {
      condition: "not-running",
    }),
  );
  while let Some(status) = stream.next().await {
    let status = status.map_err(|error| {
      IoError::interrupted("CargoInitContainer", &error.to_string())
    })?;
    if status.status_code != 0 {
      let message = status
        .error
        .and_then(|error| error.message)
        .unwrap_or_else(|| "Unknown init-container error".to_owned());
      return Err(IoError::interrupted(
        "CargoInitContainer",
        &format!("{message} (exit {})", status.status_code),
      ));
    }
  }
  Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitContainerDisposition {
  Complete,
  Start,
  Wait,
}

fn init_container_disposition(
  running: bool,
  status: Option<&str>,
  exit_code: Option<i64>,
) -> InitContainerDisposition {
  if running {
    return InitContainerDisposition::Wait;
  }
  match status {
    Some("exited") if exit_code == Some(0) => {
      InitContainerDisposition::Complete
    }
    _ => InitContainerDisposition::Start,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn init_container_must_run_before_exit_zero_counts_as_complete() {
    assert_eq!(
      init_container_disposition(false, Some("created"), Some(0)),
      InitContainerDisposition::Start
    );
    assert_eq!(
      init_container_disposition(true, Some("running"), Some(0)),
      InitContainerDisposition::Wait
    );
    assert_eq!(
      init_container_disposition(false, Some("exited"), Some(0)),
      InitContainerDisposition::Complete
    );
    assert_eq!(
      init_container_disposition(false, Some("exited"), Some(1)),
      InitContainerDisposition::Start
    );
  }
}
