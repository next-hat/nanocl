use nanocl_error::{
  io::FromIo,
  http::{HttpResult, HttpError},
};

use bollard_next::container::{
  StartContainerOptions, Config, CreateContainerOptions,
  InspectContainerOptions, StopContainerOptions, RemoveContainerOptions,
};

use nanocl_stubs::{
  system::NativeEventAction,
  process::{Process, ProcessKind, ProcessPartial},
};

use crate::{
  repositories::generic::*,
  models::{SystemState, ProcessDb, JobDb, JobUpdateDb, VmDb, CargoDb},
};

async fn after(
  kind: &ProcessKind,
  kind_key: &str,
  action: NativeEventAction,
  state: &SystemState,
) -> HttpResult<()> {
  match kind {
    ProcessKind::Vm => {
      let vm = VmDb::transform_read_by_pk(kind_key, &state.pool).await?;
      super::event_emitter::emit_normal_native_action(&vm, action, state);
    }
    ProcessKind::Cargo => {
      let cargo = CargoDb::transform_read_by_pk(kind_key, &state.pool).await?;
      super::event_emitter::emit_normal_native_action(&cargo, action, state);
    }
    ProcessKind::Job => {
      JobDb::update_pk(
        kind_key,
        JobUpdateDb {
          updated_at: Some(chrono::Utc::now().naive_utc()),
        },
        &state.pool,
      )
      .await?;
    }
  }
  Ok(())
}

pub(crate) fn parse_kind(kind: &str) -> HttpResult<ProcessKind> {
  kind.to_owned().try_into().map_err(|err: std::io::Error| {
    HttpError::internal_server_error(err.to_string())
  })
}

/// Count the number of instances running failing or success
pub(crate) fn count_status(
  instances: &[Process],
) -> (usize, usize, usize, usize) {
  let mut instance_failed = 0;
  let mut instance_success = 0;
  let mut instance_running = 0;
  for instance in instances {
    let container = &instance.data;
    let state = container.state.clone().unwrap_or_default();
    if state.restarting.unwrap_or_default() {
      instance_failed += 1;
      continue;
    }
    if state.running.unwrap_or_default() {
      instance_running += 1;
      continue;
    }
    if let Some(exit_code) = state.exit_code {
      if exit_code == 0 {
        instance_success += 1;
      } else {
        instance_failed += 1;
      }
    }
    if let Some(error) = state.error {
      if !error.is_empty() {
        instance_failed += 1;
      }
    }
  }
  (
    instances.len(),
    instance_failed,
    instance_success,
    instance_running,
  )
}

pub(crate) async fn create(
  name: &str,
  kind: &str,
  kind_key: &str,
  item: Config,
  state: &SystemState,
) -> HttpResult<Process> {
  let kind: ProcessKind =
    kind.to_owned().try_into().map_err(|err: std::io::Error| {
      HttpError::internal_server_error(err.to_string())
    })?;
  let mut config = item.clone();
  let mut labels = item.labels.to_owned().unwrap_or_default();
  labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
  labels.insert("io.nanocl.kind".to_owned(), kind.to_string());
  config.labels = Some(labels);
  let res = state
    .docker_api
    .create_container(
      Some(CreateContainerOptions {
        name,
        ..Default::default()
      }),
      config,
    )
    .await?;
  let inspect = state
    .docker_api
    .inspect_container(&res.id, None::<InspectContainerOptions>)
    .await?;
  let created_at = inspect.created.clone().unwrap_or_default();
  let new_instance = ProcessPartial {
    key: res.id,
    name: name.to_owned(),
    kind,
    data: serde_json::to_value(&inspect)
      .map_err(|err| err.map_err_context(|| "CreateProcess"))?,
    node_key: state.config.hostname.clone(),
    kind_key: kind_key.to_owned(),
    created_at: Some(
      chrono::NaiveDateTime::parse_from_str(
        &created_at,
        "%Y-%m-%dT%H:%M:%S%.fZ",
      )
      .map_err(|err| {
        HttpError::internal_server_error(format!("Unable to parse date {err}"))
      })?,
    ),
  };
  let process = ProcessDb::create_from(&new_instance, &state.pool).await?;
  Process::try_from(process)
    .map_err(|err| HttpError::internal_server_error(err.to_string()))
}

pub(crate) async fn remove(
  key: &str,
  opts: Option<RemoveContainerOptions>,
  state: &SystemState,
) -> HttpResult<()> {
  match state.docker_api.remove_container(key, opts).await {
    Ok(_) => {}
    Err(err) => match &err {
      bollard_next::errors::Error::DockerResponseServerError {
        status_code,
        message: _,
      } => {
        if *status_code != 404 {
          return Err(err.into());
        }
      }
      _ => {
        return Err(err.into());
      }
    },
  };
  ProcessDb::del_by_pk(key, &state.pool).await?;
  Ok(())
}

pub(crate) async fn start_by_kind(
  kind: &ProcessKind,
  kind_key: &str,
  state: &SystemState,
) -> HttpResult<()> {
  let processes = ProcessDb::read_by_kind_key(kind_key, &state.pool).await?;
  log::debug!("process::start_by_kind: {kind_key}");
  for process in processes {
    let process_state = process.data.state.unwrap_or_default();
    if process_state.running.unwrap_or_default() {
      return Ok(());
    }
    state
      .docker_api
      .start_container(
        &process.data.id.unwrap_or_default(),
        None::<StartContainerOptions<String>>,
      )
      .await?;
  }
  after(kind, kind_key, NativeEventAction::Start, state).await?;
  Ok(())
}

pub(crate) async fn stop_by_kind(
  kind: &ProcessKind,
  kind_key: &str,
  state: &SystemState,
) -> HttpResult<()> {
  let processes = ProcessDb::read_by_kind_key(kind_key, &state.pool).await?;
  log::debug!("process::stop_by_kind: {kind:#?} {kind_key}");
  for process in processes {
    let process_state = process.data.state.unwrap_or_default();
    if !process_state.running.unwrap_or_default() {
      return Ok(());
    }
    state
      .docker_api
      .stop_container(
        &process.data.id.unwrap_or_default(),
        None::<StopContainerOptions>,
      )
      .await?;
  }
  after(kind, kind_key, NativeEventAction::Stop, state).await?;
  Ok(())
}
