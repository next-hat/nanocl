use ntex::rt;
use ntex::http;
use ntex::util::Bytes;
use ntex::channel::mpsc;
use ntex::channel::mpsc::Receiver;

use nanocl_utils::http_error::HttpError;

use nanocl_stubs::system::Event;
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateResources, StateMeta, StateStream,
};

use crate::{utils, repositories};
use crate::models::{StateData, DaemonState};

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
) -> Result<Receiver<Result<Bytes, HttpError>>, HttpError> {
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  let data = data.clone();
  let version = version.to_owned();
  let state = state.clone();

  rt::spawn(async move {
    // If we have a namespace and it doesn't exist, create it
    // Unless we use `global` as default for the creation of cargoes
    let namespace = if let Some(namespace) = &data.namespace {
      utils::namespace::create_if_not_exists(namespace, &state).await?;
      namespace.to_owned()
    } else {
      "global".into()
    };

    if let Some(cargoes) = data.cargoes {
      if sx
        .send(stream_to_bytes(StateStream::Msg(format!(
          "Creating {0} cargoes in namespace: {namespace}",
          cargoes.len()
        ))))
        .is_err()
      {
        // TODO: Delete namespace if it was created and it's not global
        log::warn!("User stopped the deployment");
        return Ok(());
      };
      for cargo in &cargoes {
        let res =
          utils::cargo::create_or_put(&namespace, cargo, &version, &state)
            .await;

        if let Err(err) = res {
          if sx
            .send(utils::state::stream_to_bytes(StateStream::Error(
              err.to_string(),
            )))
            .is_err()
          {
            // TODO: Delete previously created cargoes
            log::warn!("User stopped the deployment");
            return Ok(());
          };
          continue;
        }

        if sx
          .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
            "Cargo {0} created",
            cargo.name
          ))))
          .is_err()
        {
          // TODO: Delete previously created cargoes
          log::warn!("User stopped the deployment");
          break;
        };

        let key = utils::key::gen_key(&namespace, &cargo.name);
        let state_ptr = state.clone();
        rt::spawn(async move {
          let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
          let _ = state_ptr
            .event_emitter
            .emit(Event::CargoPatched(Box::new(cargo)))
            .await;
        });
        let res = utils::cargo::start(
          &utils::key::gen_key(&namespace, &cargo.name),
          &state,
        )
        .await;

        if let Err(err) = res {
          if sx
            .send(utils::state::stream_to_bytes(StateStream::Error(
              err.to_string(),
            )))
            .is_err()
          {
            // TODO: Delete previously created cargoes
            log::warn!("User stopped the deployment");
            return Ok(());
          };
          continue;
        }

        if sx
          .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
            "Cargo {0} started",
            cargo.name
          ))))
          .is_err()
        {
          // TODO: Delete previously created cargoes
          log::warn!("User stopped the deployment");
          break;
        };

        let key = utils::key::gen_key(&namespace, &cargo.name);
        let state_ptr = state.clone();
        rt::spawn(async move {
          let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
          let _ = state_ptr
            .event_emitter
            .emit(Event::CargoStarted(Box::new(cargo)))
            .await;
        });
      }
    }

    if let Some(resources) = &data.resources {
      for resource in resources {
        let key = resource.name.to_owned();
        let res =
          utils::resource::create_or_patch(resource.clone(), &state.pool).await;

        if let Err(err) = res {
          if sx
            .send(utils::state::stream_to_bytes(StateStream::Error(
              err.to_string(),
            )))
            .is_err()
          {
            // TODO: Delete previously created resources
            log::warn!("User stopped the deployment");
            return Ok(());
          }
          continue;
        }

        if sx
          .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
            "Resource {0} created",
            resource.name
          ))))
          .is_err()
        {
          // TODO: Delete previously created cargoes
          log::warn!("User stopped the deployment");
          break;
        };

        let state_ptr = state.clone();
        rt::spawn(async move {
          let item =
            repositories::resource::inspect_by_key(&key, &state_ptr.pool)
              .await
              .unwrap();
          let _ = state_ptr
            .event_emitter
            .emit(Event::ResourcePatched(Box::new(item)))
            .await;
        });
      }
    }
    Ok::<_, HttpError>(())
  });

  Ok(rx)
}

pub async fn apply_cargo(
  data: &StateCargo,
  version: &str,
  state: &DaemonState,
) -> Result<Receiver<Result<Bytes, HttpError>>, HttpError> {
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  let data = data.clone();
  let version = version.to_owned();
  let state = state.clone();

  rt::spawn(async move {
    // If we have a namespace and it doesn't exist, create it
    // Unless we use `global` as default for the creation of cargoes
    let namespace = if let Some(namespace) = &data.namespace {
      utils::namespace::create_if_not_exists(namespace, &state).await?;
      namespace.to_owned()
    } else {
      "global".into()
    };

    if sx
      .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
        "Creating {0} cargoes in namespace: {namespace}",
        data.cargoes.len(),
      ))))
      .is_err()
    {
      // TODO: Delete namespace if it was created and it's not global
      log::warn!("User stopped the deployment");
      return Ok(());
    };

    for cargo in &data.cargoes {
      let res =
        utils::cargo::create_or_put(&namespace, cargo, &version, &state).await;

      if let Err(err) = res {
        if sx
          .send(utils::state::stream_to_bytes(StateStream::Error(
            err.to_string(),
          )))
          .is_err()
        {
          // TODO: Delete previously created cargoes
          log::warn!("User stopped the deployment");
          return Ok(());
        };
        continue;
      }

      if sx
        .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
          "Created cargo {0}",
          cargo.name
        ))))
        .is_err()
      {
        // TODO: Delete previously created cargoes
        log::warn!("User stopped the deployment");
        break;
      };

      let key = utils::key::gen_key(&namespace, &cargo.name);
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoPatched(Box::new(cargo)))
          .await;
      });
      let res = utils::cargo::start(
        &utils::key::gen_key(&namespace, &cargo.name),
        &state,
      )
      .await;
      if let Err(err) = res {
        if sx
          .send(utils::state::stream_to_bytes(StateStream::Error(
            err.to_string(),
          )))
          .is_err()
        {
          // TODO: Delete previously created cargoes
          log::warn!("User stopped the deployment");
          return Ok(());
        };
        continue;
      }

      if sx
        .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
          "Started cargo {0}",
          cargo.name
        ))))
        .is_err()
      {
        // TODO: Delete previously created cargoes
        log::warn!("User stopped the deployment");
        break;
      };
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoStarted(Box::new(cargo)))
          .await;
      });
    }
    Ok::<_, HttpError>(())
  });

  Ok(rx)
}

pub async fn apply_resource(
  data: &StateResources,
  state: &DaemonState,
) -> Result<Receiver<Result<Bytes, HttpError>>, HttpError> {
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  let data = data.clone();
  let state = state.clone();

  rt::spawn(async move {
    if sx
      .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
        "Creating {0} resources",
        data.resources.len(),
      ))))
      .is_err()
    {
      log::warn!("User stopped the deployment");
      return Ok(());
    };

    for resource in &data.resources {
      let key = resource.name.to_owned();
      let res =
        utils::resource::create_or_patch(resource.clone(), &state.pool).await;

      if let Err(err) = res {
        if sx
          .send(utils::state::stream_to_bytes(StateStream::Error(
            err.to_string(),
          )))
          .is_err()
        {
          // TODO: Delete previously created resources
          log::warn!("User stopped the deployment");
          return Ok(());
        }
        continue;
      }

      if sx
        .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
          "Resource {0} created",
          resource.name
        ))))
        .is_err()
      {
        log::warn!("User stopped the deployment");
        break;
      }

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
    }
    Ok::<_, HttpError>(())
  });

  Ok(rx)
}

pub async fn revert_deployment(
  data: &StateDeployment,
  state: &DaemonState,
) -> Result<Receiver<Result<Bytes, HttpError>>, HttpError> {
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  let data = data.clone();
  let state = state.clone();

  rt::spawn(async move {
    let namespace = if let Some(namespace) = &data.namespace {
      namespace.to_owned()
    } else {
      "global".into()
    };

    if let Some(cargoes) = &data.cargoes {
      if sx
        .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
          "Deleting {0} cargoes in namespace {namespace}",
          cargoes.len(),
        ))))
        .is_err()
      {
        log::warn!("User stopped the deployment");
        return Ok(());
      };

      for cargo in cargoes {
        let key = utils::key::gen_key(&namespace, &cargo.name);

        let cargo = match utils::cargo::inspect(&key, &state).await {
          Ok(cargo) => cargo,
          Err(_) => {
            if sx
              .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
                "Cargo {0} not found skipping",
                cargo.name
              ))))
              .is_err()
            {
              log::warn!("User stopped the deployment");
              break;
            }
            continue;
          }
        };

        utils::cargo::delete(&key, Some(true), &state).await?;

        if sx
          .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
            "Cargo {0} deleted",
            cargo.name
          ))))
          .is_err()
        {
          log::warn!("User stopped the deployment");
          break;
        }

        let state_ptr = state.clone();
        rt::spawn(async move {
          let _ = state_ptr
            .event_emitter
            .emit(Event::CargoDeleted(Box::new(cargo)))
            .await;
          Ok::<_, HttpError>(())
        });
      }
    }

    if let Some(resources) = &data.resources {
      if sx
        .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
          "Deleting {0} resources",
          resources.len(),
        ))))
        .is_err()
      {
        log::warn!("User stopped the deployment");
        return Ok(());
      };

      for resource in resources {
        let key = resource.name.to_owned();
        let resource =
          match repositories::resource::inspect_by_key(&key, &state.pool).await
          {
            Ok(resource) => resource,
            Err(_) => {
              if sx
                .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
                  "Resource {0} not found skipping",
                  resource.name
                ))))
                .is_err()
              {
                log::warn!("User stopped the deployment");
                return Ok(());
              }
              continue;
            }
          };
        utils::resource::delete(resource.clone(), &state.pool).await?;
        if sx
          .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
            "Resource {0} deleted",
            resource.name
          ))))
          .is_err()
        {
          log::warn!("User stopped the deployment");
          break;
        }
        let state_ptr = state.clone();
        rt::spawn(async move {
          let _ = state_ptr
            .event_emitter
            .emit(Event::ResourceDeleted(Box::new(resource)))
            .await;
        });
      }
    }
    Ok::<_, HttpError>(())
  });
  Ok(rx)
}

pub async fn revert_cargo(
  data: &StateCargo,
  state: &DaemonState,
) -> Result<Receiver<Result<Bytes, HttpError>>, HttpError> {
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  let data = data.clone();
  let state = state.clone();

  rt::spawn(async move {
    let namespace = if let Some(namespace) = &data.namespace {
      namespace.to_owned()
    } else {
      "global".into()
    };

    if sx
      .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
        "Deleting {0} cargoes in namespace {namespace}",
        data.cargoes.len(),
      ))))
      .is_err()
    {
      log::warn!("User stopped the deployment");
      return Ok(());
    };

    for cargo in &data.cargoes {
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let cargo = match utils::cargo::inspect(&key, &state).await {
        Ok(cargo) => cargo,
        Err(_) => {
          if sx
            .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
              "Cargo {0} not found",
              cargo.name
            ))))
            .is_err()
          {
            log::warn!("User stopped the deployment");
            break;
          }
          continue;
        }
      };
      utils::cargo::delete(&key, Some(true), &state).await?;
      if sx
        .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
          "Cargo {0} deleted",
          cargo.name
        ))))
        .is_err()
      {
        log::warn!("User stopped the deployment");
        break;
      }
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::CargoDeleted(Box::new(cargo)))
          .await;
      });
    }

    Ok::<_, HttpError>(())
  });
  Ok(rx)
}

pub async fn revert_resource(
  data: &StateResources,
  state: &DaemonState,
) -> Result<Receiver<Result<Bytes, HttpError>>, HttpError> {
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  let data = data.clone();
  let state = state.clone();

  rt::spawn(async move {
    if sx
      .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
        "Deleting {0} resources",
        data.resources.len(),
      ))))
      .is_err()
    {
      log::warn!("User stopped the deployment");
      return Ok(());
    };

    for resource in &data.resources {
      let key = resource.name.to_owned();
      let resource =
        match repositories::resource::inspect_by_key(&key, &state.pool).await {
          Ok(resource) => resource,
          Err(_) => {
            if sx
              .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
                "Resource {0} not found skipping",
                resource.name
              ))))
              .is_err()
            {
              log::warn!("User stopped the deployment");
              return Ok(());
            }
            continue;
          }
        };
      utils::resource::delete(resource.clone(), &state.pool).await?;
      if sx
        .send(utils::state::stream_to_bytes(StateStream::Msg(format!(
          "Resource {0} deleted",
          resource.name
        ))))
        .is_err()
      {
        log::warn!("User stopped the deployment");
        break;
      }
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::ResourceDeleted(Box::new(resource)))
          .await;
      });
    }
    Ok::<_, HttpError>(())
  });
  Ok(rx)
}
