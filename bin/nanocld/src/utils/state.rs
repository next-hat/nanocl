use ntex::rt;
use ntex::http;
use ntex::util::Bytes;
use ntex::channel::mpsc;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use nanocl_utils::http_error::HttpError;

use nanocl_stubs::system::Event;
use nanocl_stubs::resource::ResourcePartial;
use nanocl_stubs::cargo_config::CargoConfigPartial;
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateResources, StateMeta, StateStream,
};

use crate::{utils, repositories};
use crate::models::{StateData, DaemonState};

async fn create_cargoes(
  namespace: &str,
  data: &[CargoConfigPartial],
  version: &str,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) {
  let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(format!(
    "Creating {0} cargoes in namespace: {namespace}",
    data.len(),
  ))));
  data
    .iter()
    .map(|cargo| async {
      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Creating Cargo {0}", cargo.name),
      )));

      let key = utils::key::gen_key(namespace, &cargo.name);
      let res = match utils::cargo::inspect(&key, state).await {
        Ok(existing) => {
          let existing: CargoConfigPartial = existing.into();
          if existing == *cargo {
            let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
              format!("Skipping Cargo {0} not changed", cargo.name),
            )));
            return Ok(());
          }
          utils::cargo::put(&key, cargo, version, state).await
        }
        Err(_err) => {
          let cargo =
            utils::cargo::create(namespace, cargo, version, state).await?;
          utils::cargo::start(&key, state).await?;
          Ok(cargo)
        }
      };

      if let Err(err) = res {
        let _ = sx.send(utils::state::stream_to_bytes(StateStream::Error(
          err.to_string(),
        )));
        return Ok(());
      }

      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Created Cargo {0}", cargo.name),
      )));

      let key = utils::key::gen_key(namespace, &cargo.name);
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoPatched(Box::new(cargo)))
          .await;
      });
      let res = utils::cargo::start(
        &utils::key::gen_key(namespace, &cargo.name),
        state,
      )
      .await;

      if let Err(err) = res {
        let _ = sx.send(utils::state::stream_to_bytes(StateStream::Error(
          err.to_string(),
        )));
        return Ok(());
      }
      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Started Cargo {0}", cargo.name),
      )));
      let key = utils::key::gen_key(namespace, &cargo.name);
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoStarted(Box::new(cargo)))
          .await;
      });
      Ok::<_, HttpError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

async fn create_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) {
  let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(format!(
    "Creating {0} resources",
    data.len(),
  ))));
  data
    .iter()
    .map(|resource| async {
      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Creating Resource {0}", resource.name),
      )));
      let key = resource.name.to_owned();

      let res =
        match repositories::resource::inspect_by_key(&key, &state.pool).await {
          Err(_) => utils::resource::create(resource, &state.pool).await,
          Ok(cur_resource) => {
            let casted: ResourcePartial = cur_resource.into();
            if *resource == casted {
              let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
                format!("Skipping Resource {0} not changed", resource.name),
              )));
              return Ok(());
            }
            utils::resource::patch(&resource.clone(), &state.pool).await
          }
        };

      if let Err(err) = res {
        let _ = sx.send(utils::state::stream_to_bytes(StateStream::Error(
          err.to_string(),
        )));
        return Ok(());
      }
      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Created Resource {0}", resource.name),
      )));
      let pool = state.pool.clone();
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let resource = repositories::resource::inspect_by_key(&key, &pool)
          .await
          .unwrap();
        let _ = event_emitter
          .emit(Event::ResourcePatched(Box::new(resource)))
          .await;
      });
      Ok::<_, HttpError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

pub fn stream_to_bytes(state_stream: StateStream) -> Result<Bytes, HttpError> {
  let bytes =
    serde_json::to_string(&state_stream).map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("unable to serialize state_stream_to_bytes {err}"),
    })?;
  Ok(Bytes::from(bytes + "\r\n"))
}

pub fn parse_state(data: &serde_json::Value) -> Result<StateData, HttpError> {
  let meta =
    serde_json::from_value::<StateMeta>(data.to_owned()).map_err(|err| {
      HttpError {
        status: http::StatusCode::BAD_REQUEST,
        msg: format!("unable to serialize payload {err}"),
      }
    })?;
  match meta.kind.as_str() {
    "Deployment" => {
      let data = serde_json::from_value::<StateDeployment>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Deployment(data))
    }
    "Cargo" => {
      let data = serde_json::from_value::<StateCargo>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Cargo(data))
    }
    "Resource" => {
      let data = serde_json::from_value::<StateResources>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Resource(data))
    }
    _ => Err(HttpError {
      status: http::StatusCode::BAD_REQUEST,
      msg: "unknown type".into(),
    }),
  }
}

pub async fn apply_deployment(
  data: &StateDeployment,
  version: &str,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let data = data.clone();
  let version = version.to_owned();
  let state = state.clone();

  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(namespace, &state).await?;
    namespace.to_owned()
  } else {
    "global".into()
  };

  if let Some(cargoes) = data.cargoes {
    create_cargoes(&namespace, &cargoes, &version, &state, sx.clone()).await;
  }

  if let Some(resources) = &data.resources {
    create_resources(resources, &state, sx.clone()).await;
  }

  Ok(())
}

pub async fn apply_cargo(
  data: &StateCargo,
  version: &str,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let data = data.clone();
  let version = version.to_owned();
  let state = state.clone();
  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(namespace, &state).await?;
    namespace.to_owned()
  } else {
    "global".into()
  };
  create_cargoes(&namespace, &data.cargoes, &version, &state, sx).await;
  Ok(())
}

pub async fn apply_resource(
  data: &StateResources,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let data = data.clone();
  let state = state.clone();
  create_resources(&data.resources, &state, sx).await;
  Ok(())
}

async fn delete_cargoes(
  namespace: &str,
  data: &[CargoConfigPartial],
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) {
  let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(format!(
    "Deleting {0} cargoes in namespace {namespace}",
    data.len()
  ))));

  data
    .iter()
    .map(|cargo| async {
      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Deleting Cargo {0}", cargo.name),
      )));

      let key = utils::key::gen_key(namespace, &cargo.name);

      let res = utils::cargo::inspect(&key, state).await;
      if res.is_err() {
        let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
          format!("Skipping Cargo {0} [NOT FOUND]", cargo.name),
        )));
        return Ok(());
      }

      let cargo = res.unwrap();

      utils::cargo::delete(&key, Some(true), state).await?;

      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Deleted Cargo {0}", cargo.name),
      )));

      let event_emitter = state.event_emitter.clone();
      let state_clone = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_clone).await.unwrap();
        let _ = event_emitter
          .emit(Event::CargoDeleted(Box::new(cargo)))
          .await;
      });
      Ok::<_, HttpError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

async fn delete_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) {
  let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(format!(
    "Deleting {0} resources",
    data.len(),
  ))));

  data
    .iter()
    .map(|resource| async {
      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Deleting Resource {0}", resource.name),
      )));

      let key = resource.name.to_owned();
      let resource =
        match repositories::resource::inspect_by_key(&key, &state.pool).await {
          Ok(resource) => resource,
          Err(_) => {
            let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
              format!("Skipping Resource {0} [NOT FOUND]", resource.name),
            )));
            return Ok(());
          }
        };

      utils::resource::delete(&resource.clone(), &state.pool).await?;

      let _ = sx.send(utils::state::stream_to_bytes(StateStream::Msg(
        format!("Deleted Resource {0}", resource.name),
      )));
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::ResourceDeleted(Box::new(resource)))
          .await;
      });
      Ok::<_, HttpError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

pub async fn remove_deployment(
  data: &StateDeployment,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let data = data.clone();
  let state = state.clone();

  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };

  if let Some(cargoes) = &data.cargoes {
    delete_cargoes(&namespace, cargoes, &state, sx.clone()).await;
  }

  if let Some(resources) = &data.resources {
    delete_resources(resources, &state, sx.clone()).await;
  }

  Ok(())
}

pub async fn remove_cargo(
  data: &StateCargo,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let data = data.clone();
  let state = state.clone();

  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };

  delete_cargoes(&namespace, &data.cargoes, &state, sx).await;
  Ok(())
}

pub async fn remove_resource(
  data: &StateResources,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let data = data.clone();
  let state = state.clone();

  delete_resources(&data.resources, &state, sx).await;
  Ok(())
}
