use bollard_next::container::{
  RemoveContainerOptions, StartContainerOptions, StopContainerOptions, Config,
  CreateContainerOptions, InspectContainerOptions,
};
use nanocl_error::{
  http::{HttpResult, HttpError},
  io::FromIo,
};
use nanocl_stubs::{
  process::{ProcessKind, ProcessPartial, Process},
  system::NativeEventAction,
};

use crate::{
  repositories::generic::*,
  models::{SystemState, ProcessDb, VmDb, CargoDb, JobDb, JobUpdateDb},
};

/// Represent a object that is treated as a process
/// That you can start, restart, stop, logs, etc.
pub trait ObjProcess {
  fn get_kind() -> ProcessKind;

  async fn _emit(
    kind_key: &str,
    action: NativeEventAction,
    state: &SystemState,
  ) -> HttpResult<()> {
    match Self::get_kind() {
      ProcessKind::Vm => {
        let vm = VmDb::transform_read_by_pk(kind_key, &state.pool).await?;
        state.emit_normal_native_action(&vm, action);
      }
      ProcessKind::Cargo => {
        let cargo =
          CargoDb::transform_read_by_pk(kind_key, &state.pool).await?;
        state.emit_normal_native_action(&cargo, action);
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
        let job = JobDb::read_by_pk(kind_key, &state.pool)
          .await?
          .try_to_spec()?;
        state.emit_normal_native_action(&job, action);
      }
    }
    Ok(())
  }

  async fn create_process(
    name: &str,
    kind_key: &str,
    item: Config,
    state: &SystemState,
  ) -> HttpResult<Process> {
    let kind = Self::get_kind();
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
          HttpError::internal_server_error(format!(
            "Unable to parse date {err}"
          ))
        })?,
      ),
    };
    let process = ProcessDb::create_from(&new_instance, &state.pool).await?;
    Process::try_from(process)
      .map_err(|err| HttpError::internal_server_error(err.to_string()))
  }

  async fn start_process_by_kind_pk(
    kind_pk: &str,
    state: &SystemState,
  ) -> HttpResult<()> {
    let processes = ProcessDb::read_by_kind_key(kind_pk, &state.pool).await?;
    log::debug!("start_process_by_kind_pk: {kind_pk}");
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
    Self::_emit(kind_pk, NativeEventAction::Create, state).await?;
    Ok(())
  }

  async fn stop_process_by_kind_pk(
    kind_pk: &str,
    state: &SystemState,
  ) -> HttpResult<()> {
    let processes = ProcessDb::read_by_kind_key(kind_pk, &state.pool).await?;
    log::debug!("stop_process_by_kind_pk: {kind_pk}");
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
    Self::_emit(kind_pk, NativeEventAction::Stop, state).await?;
    Ok(())
  }

  /// Delete a process by pk
  async fn del_process_by_pk(
    pk: &str,
    opts: Option<RemoveContainerOptions>,
    state: &SystemState,
  ) -> HttpResult<()> {
    match state.docker_api.remove_container(pk, opts).await {
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
    ProcessDb::del_by_pk(pk, &state.pool).await?;
    Ok(())
  }
}
