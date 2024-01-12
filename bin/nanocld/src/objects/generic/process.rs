use futures_util::{StreamExt, stream::FuturesUnordered};
use bollard_next::container::{
  RemoveContainerOptions, StopContainerOptions, Config, CreateContainerOptions,
  InspectContainerOptions,
};
use nanocl_error::{
  io::FromIo,
  http::{HttpResult, HttpError},
};
use nanocl_stubs::{
  system::NativeEventAction,
  process::{ProcessKind, ProcessPartial, Process},
  cargo::CargoKillOptions,
};

use crate::{
  repositories::generic::*,
  models::{
    SystemState, ProcessDb, VmDb, CargoDb, JobDb, JobUpdateDb, ObjPsStatusDb,
    ObjPsStatusUpdate, ObjPsStatusKind,
  },
};

/// Represent a object that is treated as a process
/// That you can start, restart, stop, logs, etc.
pub trait ObjProcess {
  fn get_process_kind() -> ProcessKind;

  async fn _emit(
    kind_key: &str,
    action: NativeEventAction,
    state: &SystemState,
  ) -> HttpResult<()> {
    match Self::get_process_kind() {
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
    let kind = Self::get_process_kind();
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
    Process::try_from(process).map_err(HttpError::from)
  }

  async fn start_process_by_kind_key(
    kind_pk: &str,
    state: &SystemState,
  ) -> HttpResult<()> {
    log::debug!("start_process_by_kind_pk: {kind_pk}");
    let current_status =
      ObjPsStatusDb::read_by_pk(kind_pk, &state.pool).await?;
    if current_status.actual == ObjPsStatusKind::Running.to_string() {
      log::debug!("start_process_by_kind_pk: {kind_pk} already running");
      return Ok(());
    }
    let status_update = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Running.to_string()),
      prev_wanted: Some(current_status.wanted),
      actual: Some(ObjPsStatusKind::Starting.to_string()),
      prev_actual: Some(current_status.actual),
    };
    log::debug!("start_process_by_kind_pk: {kind_pk} update status");
    ObjPsStatusDb::update_pk(kind_pk, status_update, &state.pool).await?;
    Self::_emit(kind_pk, NativeEventAction::Starting, state).await?;
    log::debug!("start emitted !");
    Ok(())
  }

  async fn stop_process_by_kind_key(
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

  async fn restart_process_by_kind_key(
    pk: &str,
    state: &SystemState,
  ) -> HttpResult<()> {
    let processes = ProcessDb::read_by_kind_key(pk, &state.pool).await?;
    processes
      .into_iter()
      .map(|process| async move {
        state
          .docker_api
          .restart_container(&process.key, None)
          .await
          .map_err(HttpError::from)
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<HttpResult<()>>>()
      .await
      .into_iter()
      .collect::<HttpResult<Vec<_>>>()?;
    Self::_emit(pk, NativeEventAction::Restart, state).await?;
    Ok(())
  }

  async fn kill_process_by_kind_key(
    pk: &str,
    opts: &CargoKillOptions,
    state: &SystemState,
  ) -> HttpResult<()> {
    let processes = ProcessDb::read_by_kind_key(pk, &state.pool).await?;
    processes
      .into_iter()
      .map(|process| async move {
        let id = process.data.id.clone().unwrap_or_default();
        let options = opts.clone().into();
        state
          .docker_api
          .kill_container(&id, Some(options))
          .await
          .map_err(HttpError::from)
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<HttpResult<()>>>()
      .await
      .into_iter()
      .collect::<HttpResult<Vec<_>>>()?;
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
