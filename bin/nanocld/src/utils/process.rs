use bollard_next::container::{
  StartContainerOptions, Config, CreateContainerOptions,
  InspectContainerOptions, StopContainerOptions, RemoveContainerOptions,
};

use nanocl_error::{
  http::{HttpResult, HttpError},
  io::FromIo,
};
use nanocl_stubs::{
  system::EventAction,
  generic::{GenericFilter, GenericClause},
};

use crate::models::{
  DaemonState, Repository, ProcessDb, JobDb, JobUpdateDb, ProcessPartial,
  Process, ProcessKind, VmDb, CargoDb,
};

async fn after(
  kind: &ProcessKind,
  kind_key: &str,
  action: EventAction,
  state: &DaemonState,
) -> HttpResult<()> {
  let filter =
    GenericFilter::new().r#where("key", GenericClause::Eq(kind_key.to_owned()));
  match kind {
    ProcessKind::Vm => {
      let vm = VmDb::find_one(&filter, &state.pool).await??;
      state.event_emitter.spawn_emit_to_event(&vm, action);
    }
    ProcessKind::Cargo => {
      let vm = CargoDb::find_one(&filter, &state.pool).await??;
      state.event_emitter.spawn_emit_to_event(&vm, action);
    }
    ProcessKind::Job => {
      JobDb::update_by_pk(
        kind_key,
        JobUpdateDb {
          updated_at: Some(chrono::Utc::now().naive_utc()),
        },
        &state.pool,
      )
      .await??;
    }
  }
  Ok(())
}

pub(crate) async fn create(
  name: &str,
  kind: &str,
  kind_key: &str,
  item: Config,
  state: &DaemonState,
) -> HttpResult<Process> {
  let kind: ProcessKind = kind.to_owned().try_into()?;
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
  let new_instance = ProcessPartial {
    key: res.id,
    name: name.to_owned(),
    kind,
    data: serde_json::to_value(&inspect)
      .map_err(|err| err.map_err_context(|| "CreateProcess"))?,
    node_key: state.config.hostname.clone(),
    kind_key: kind_key.to_owned(),
  };
  let process = ProcessDb::create(&new_instance, &state.pool).await??;
  Process::try_from(process)
    .map_err(|err| HttpError::internal_server_error(err.to_string()))
}

pub(crate) async fn remove(
  key: &str,
  opts: Option<RemoveContainerOptions>,
  state: &DaemonState,
) -> HttpResult<()> {
  state.docker_api.remove_container(key, opts).await?;
  ProcessDb::delete_by_pk(key, &state.pool).await??;
  Ok(())
}

pub(crate) async fn start_by_kind(
  kind: &ProcessKind,
  kind_key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let processes = ProcessDb::find_by_kind_key(kind_key, &state.pool).await?;
  log::debug!("process::start_by_kind: {processes:#?}");
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
  after(kind, kind_key, EventAction::Started, state).await?;
  Ok(())
}

pub(crate) async fn stop_by_kind(
  kind: &ProcessKind,
  kind_key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let processes = ProcessDb::find_by_kind_key(kind_key, &state.pool).await?;
  log::debug!("process::stop_by_kind: {processes:#?}");
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
  after(kind, kind_key, EventAction::Stopped, state).await?;
  Ok(())
}