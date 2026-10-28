use bollard_next::{
  errors::Error as DockerError, models::ContainerWaitResponse,
  query_parameters::WaitContainerOptions,
};
use futures::{Stream, StreamExt};
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{cargo::Cargo, node::CargoReplicaTask, process::Process};

use crate::models::{
  CargoReplicaProcessRole, CargoRuntimeContext, CompileDeclaredOpts,
  EnsureExistingSlotOpts,
};

use super::{
  RuntimeSlot, compile_declared, create_process, ensure_existing_slot,
  ensure_mapping, inspect_process, start_process,
};

#[cfg(test)]
pub(crate) use wait_fault::{fail_next_init_wait, pause_next_init_start};

pub(super) async fn reconcile_containers(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  attempt_id: &str,
  gateway: &str,
  slots: &mut Vec<RuntimeSlot>,
  state: &CargoRuntimeContext,
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
  state: &CargoRuntimeContext,
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

async fn wait_init(
  process: &Process,
  state: &CargoRuntimeContext,
) -> IoResult<()> {
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
    InitContainerDisposition::Start => {
      #[cfg(test)]
      wait_fault::before_start(&process.kind_key, &process.key).await?;
      start_process(process, state).await?;
    }
    InitContainerDisposition::Wait => {}
  }
  #[cfg(test)]
  if wait_fault::take(&process.kind_key) {
    return wait_init_outcome(futures::stream::iter([Err(
      DockerError::DockerResponseServerError {
        status_code: 500,
        message: "injected unavailable init wait response".to_owned(),
      },
    )]))
    .await;
  }
  let stream = state.inner.docker_api.wait_container(
    &process.key,
    Some(WaitContainerOptions {
      condition: "not-running".to_owned(),
    }),
  );
  wait_init_outcome(stream).await
}

async fn wait_init_outcome(
  mut stream: impl Stream<Item = Result<ContainerWaitResponse, DockerError>> + Unpin,
) -> IoResult<()> {
  match stream.next().await {
    Some(Ok(ContainerWaitResponse {
      status_code: code, ..
    }))
    | Some(Err(DockerError::DockerContainerWaitError { code, .. }))
      if code > 0 =>
    {
      // Bollard reports confirmed nonzero exits as structured errors.
      // Keep the reason useful without retaining arbitrary Docker messages.
      Err(IoError::interrupted(
        "CargoInitContainerExit",
        &format!("Init container exited with code {code}"),
      ))
    }
    Some(Ok(ContainerWaitResponse {
      status_code: 0,
      error: None,
    })) => Ok(()),
    Some(Err(error)) => Err(IoError::interrupted(
      "CargoInitContainer",
      &error.to_string(),
    )),
    Some(Ok(_)) | None => Err(IoError::interrupted(
      "CargoInitContainer",
      "Docker wait did not establish an init-container exit result",
    )),
  }
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
mod wait_fault {
  use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
  };

  use nanocl_error::io::{IoError, IoResult};
  use tokio::sync::oneshot;

  enum Hook {
    WaitFailure(bool),
    BeforeStart(oneshot::Sender<String>, oneshot::Receiver<()>),
  }

  static FAULTS: LazyLock<Mutex<HashMap<String, Hook>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

  pub(crate) struct InitWaitFaultGuard {
    key: String,
  }

  impl Drop for InitWaitFaultGuard {
    fn drop(&mut self) {
      FAULTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&self.key);
    }
  }

  pub(crate) fn fail_next_init_wait(key: &str) -> InitWaitFaultGuard {
    let mut faults = FAULTS.lock().expect("init wait fault lock");
    assert!(
      !faults.contains_key(key),
      "init wait fault already registered"
    );
    faults.insert(key.to_owned(), Hook::WaitFailure(true));
    InitWaitFaultGuard {
      key: key.to_owned(),
    }
  }

  pub(super) fn take(key: &str) -> bool {
    let mut faults = FAULTS.lock().expect("init wait fault lock");
    match faults.get_mut(key) {
      Some(Hook::WaitFailure(pending)) => std::mem::take(pending),
      _ => false,
    }
  }

  pub(crate) struct InitStartBarrier {
    _guard: InitWaitFaultGuard,
    entered: oneshot::Receiver<String>,
    release: Option<oneshot::Sender<()>>,
  }

  impl InitStartBarrier {
    pub(crate) async fn wait_start(&mut self) -> String {
      (&mut self.entered)
        .await
        .expect("init start barrier dropped")
    }

    pub(crate) fn release(mut self) {
      let _ = self.release.take().unwrap().send(());
    }
  }

  pub(crate) fn pause_next_init_start(key: &str) -> InitStartBarrier {
    let (entered, receiver) = oneshot::channel();
    let (release, resume) = oneshot::channel();
    let mut faults = FAULTS.lock().expect("init wait fault lock");
    assert!(
      !faults.contains_key(key),
      "init wait fault already registered"
    );
    faults.insert(key.to_owned(), Hook::BeforeStart(entered, resume));
    InitStartBarrier {
      _guard: InitWaitFaultGuard {
        key: key.to_owned(),
      },
      entered: receiver,
      release: Some(release),
    }
  }

  pub(super) async fn before_start(key: &str, id: &str) -> IoResult<()> {
    let hook = {
      let mut faults = FAULTS.lock().expect("init wait fault lock");
      if matches!(faults.get(key), Some(Hook::BeforeStart(..))) {
        faults.remove(key)
      } else {
        None
      }
    };
    if let Some(Hook::BeforeStart(entered, release)) = hook {
      let _ = entered.send(id.to_owned());
      release.await.map_err(|_| {
        IoError::interrupted(
          "CargoInitContainer",
          "init start barrier cancelled",
        )
      })?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use bollard_next::models::ContainerWaitExitError;
  use futures::{FutureExt, executor::block_on, stream};

  use super::*;

  #[test]
  fn init_wait_fault_is_one_shot_scoped_and_cleared_on_drop() {
    let key = format!("global.init-wait-fault-{}", uuid::Uuid::new_v4());
    let guard = fail_next_init_wait(&key);
    assert!(!wait_fault::take("global.other-init-wait"));
    assert!(wait_fault::take(&key));
    assert!(!wait_fault::take(&key));
    drop(guard);

    let guard = fail_next_init_wait(&key);
    drop(guard);
    assert!(!wait_fault::take(&key));
  }

  #[test]
  fn init_start_barrier_is_scoped_one_shot_and_cancels_on_drop() {
    let key = format!("global.init-start-barrier-{}", uuid::Uuid::new_v4());
    let mut barrier = pause_next_init_start(&key);
    assert!(
      block_on(wait_fault::before_start("global.other", "other")).is_ok()
    );
    let start = wait_fault::before_start(&key, "candidate-id");
    futures::pin_mut!(start);
    assert!(start.as_mut().now_or_never().is_none());
    assert_eq!(block_on(barrier.wait_start()), "candidate-id");
    barrier.release();
    assert!(block_on(start).is_ok());
    assert!(block_on(wait_fault::before_start(&key, "next-id")).is_ok());

    let barrier = pause_next_init_start(&key);
    let start = wait_fault::before_start(&key, "cancelled-id");
    futures::pin_mut!(start);
    assert!(start.as_mut().now_or_never().is_none());
    drop(barrier);
    assert!(block_on(start).is_err());
    drop(pause_next_init_start(&key));
    assert!(block_on(wait_fault::before_start(&key, "after-drop")).is_ok());
  }

  #[test]
  fn init_wait_completes_on_confirmed_zero_exit() {
    let results = stream::iter([Ok(ContainerWaitResponse {
      status_code: 0,
      error: None,
    })])
    .chain(stream::pending());
    assert!(
      wait_init_outcome(results)
        .now_or_never()
        .expect("a confirmed exit must not require another stream result")
        .is_ok()
    );
  }

  #[test]
  fn init_wait_classifies_confirmed_nonzero_exits() {
    let message = "untrusted Docker detail".to_owned();
    for result in [
      Ok(ContainerWaitResponse {
        status_code: 17,
        error: None,
      }),
      Ok(ContainerWaitResponse {
        status_code: 17,
        error: Some(ContainerWaitExitError {
          message: Some(message.clone()),
        }),
      }),
      Err(DockerError::DockerContainerWaitError {
        code: 17,
        error: message,
      }),
    ] {
      let error = block_on(wait_init_outcome(stream::iter([result])))
        .expect_err("a confirmed nonzero exit must fail");
      assert_eq!(error.context(), Some("CargoInitContainerExit"));
      assert_eq!(
        error.inner.to_string(),
        "Init container exited with code 17"
      );
    }
  }

  #[test]
  fn init_wait_keeps_unknown_outcomes_uncertain() {
    for result in [
      Some(Err(DockerError::RequestTimeoutError)),
      Some(Err(DockerError::DockerResponseServerError {
        status_code: 500,
        message: "Docker unavailable".to_owned(),
      })),
      Some(Err(DockerError::DockerContainerWaitError {
        code: 0,
        error: "wait failed without an exit status".to_owned(),
      })),
      Some(Ok(ContainerWaitResponse {
        status_code: -1,
        error: None,
      })),
      Some(Ok(ContainerWaitResponse {
        status_code: 0,
        error: Some(ContainerWaitExitError {
          message: Some("wait canceled".to_owned()),
        }),
      })),
      None,
    ] {
      let error = block_on(wait_init_outcome(stream::iter(result)))
        .expect_err("missing exit evidence must remain uncertain");
      assert_eq!(error.context(), Some("CargoInitContainer"));
    }
  }

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
