use std::collections::HashMap;

use ntex::rt;
use ntex::http::StatusCode;

use bollard_next::service::HostConfig;

use nanocl_stubs::system::Event;
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateResources, StateConfig,
};
use nanocl_stubs::cargo_config::{CargoConfigPartial, ContainerConfig};

use crate::{utils, repositories};
use crate::error::{HttpError, CliError};
use crate::models::{StateData, DaemonState};

pub fn parse_state(data: &serde_json::Value) -> Result<StateData, HttpError> {
  let meta =
    serde_json::from_value::<StateConfig>(data.to_owned()).map_err(|err| {
      HttpError {
        status: StatusCode::BAD_REQUEST,
        msg: format!("unable to serialize payload {err}"),
      }
    })?;
  match meta.r#type.as_str() {
    "Deployment" => {
      let data = serde_json::from_value::<StateDeployment>(data.to_owned())
        .map_err(|err| HttpError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Deployment(data))
    }
    "Cargo" => {
      let data = serde_json::from_value::<StateCargo>(data.to_owned())
        .map_err(|err| HttpError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Cargo(data))
    }
    "Resource" => {
      let data = serde_json::from_value::<StateResources>(data.to_owned())
        .map_err(|err| HttpError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Resource(data))
    }
    _ => Err(HttpError {
      status: StatusCode::BAD_REQUEST,
      msg: "unknown type".into(),
    }),
  }
}

pub async fn apply_deployment(
  data: &StateDeployment,
  version: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(namespace, state).await?;
    namespace.to_owned()
  } else {
    "global".into()
  };

  if let Some(cargoes) = &data.cargoes {
    for cargo in cargoes {
      utils::cargo::create_or_put(&namespace, cargo, version, state).await?;
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoPatched(Box::new(cargo)))
          .await;
      });
      utils::cargo::start(&utils::key::gen_key(&namespace, &cargo.name), state)
        .await?;
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
      utils::resource::create_or_patch(resource.clone(), &state.pool).await?;
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

  Ok(())
}

pub async fn apply_cargo(
  data: &StateCargo,
  version: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(namespace, state).await?;
    namespace.to_owned()
  } else {
    "global".into()
  };

  for cargo in &data.cargoes {
    utils::cargo::create_or_put(&namespace, cargo, version, state).await?;
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let state_ptr = state.clone();
    rt::spawn(async move {
      let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
      let _ = state_ptr
        .event_emitter
        .emit(Event::CargoPatched(Box::new(cargo)))
        .await;
    });
    utils::cargo::start(&utils::key::gen_key(&namespace, &cargo.name), state)
      .await?;
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

  Ok(())
}

pub async fn apply_resource(
  data: &StateResources,
  state: &DaemonState,
) -> Result<(), HttpError> {
  for resource in &data.resources {
    let key = resource.name.to_owned();
    utils::resource::create_or_patch(resource.clone(), &state.pool).await?;
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
  Ok(())
}

pub async fn revert_deployment(
  data: &StateDeployment,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };

  if let Some(cargoes) = &data.cargoes {
    for cargo in cargoes {
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let cargo = utils::cargo::inspect(&key, state).await?;
      utils::cargo::delete(&key, Some(true), state).await?;
      let state_ptr = state.clone();
      rt::spawn(async move {
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoDeleted(Box::new(cargo)))
          .await;
      });
    }
  }

  if let Some(resources) = &data.resources {
    for resource in resources {
      let key = resource.name.to_owned();
      let resource =
        repositories::resource::inspect_by_key(&key, &state.pool).await?;
      utils::resource::delete(resource.clone(), &state.pool).await?;
      let state_ptr = state.clone();
      rt::spawn(async move {
        let _ = state_ptr
          .event_emitter
          .emit(Event::ResourceDeleted(Box::new(resource)))
          .await;
      });
    }
  }

  Ok(())
}

pub async fn revert_cargo(
  data: &StateCargo,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };

  for cargo in &data.cargoes {
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let cargo = utils::cargo::inspect(&key, state).await?;
    utils::cargo::delete(&key, Some(true), state).await?;
    let event_emitter = state.event_emitter.clone();
    rt::spawn(async move {
      let _ = event_emitter
        .emit(Event::CargoDeleted(Box::new(cargo)))
        .await;
    });
  }

  Ok(())
}

pub async fn revert_resource(
  data: &StateResources,
  state: &DaemonState,
) -> Result<(), HttpError> {
  for resource in &data.resources {
    let key = resource.name.to_owned();
    let resource =
      repositories::resource::inspect_by_key(&key, &state.pool).await?;
    utils::resource::delete(resource.clone(), &state.pool).await?;
    let event_emitter = state.event_emitter.clone();
    rt::spawn(async move {
      let _ = event_emitter
        .emit(Event::ResourceDeleted(Box::new(resource)))
        .await;
    });
  }
  Ok(())
}

pub fn hook_cargo_binds(
  cargo: &CargoConfigPartial,
) -> Result<CargoConfigPartial, CliError> {
  if let Some(host_config) = &cargo.container.host_config {
    if let Some(binds) = &host_config.binds {
      let mut new_bind = Vec::new();
      for bind in binds {
        let split = bind.split(':').collect::<Vec<&str>>();
        let source = split.first();
        let dest = split.get(1);
        let dest = dest.unwrap_or(&"");
        let source = source.unwrap_or(&"");
        if source.starts_with('.') {
          let cwd = std::env::current_dir().map_err(|err| CliError {
            msg: format!("Failed to get current directory: {}", err),
            code: 5,
          })?;
          let source = cwd
            .join(source)
            .canonicalize()
            .map_err(|err| CliError {
              msg: format!("Failed to get canonical path: {}", err),
              code: 5,
            })?
            .display()
            .to_string();
          new_bind.push(format!("{}:{}", source, dest));
          continue;
        }
        new_bind.push(bind.to_owned());
      }
      return Ok(CargoConfigPartial {
        container: ContainerConfig::<String> {
          host_config: Some(HostConfig {
            binds: Some(new_bind),
            ..host_config.clone()
          }),
          ..cargo.container.clone()
        },
        ..cargo.clone()
      });
    }
  }

  Ok(cargo.clone())
}

pub fn hook_labels(
  namespace: &str,
  cargo: &CargoConfigPartial,
) -> CargoConfigPartial {
  let key = utils::key::gen_key(namespace, &cargo.name);

  let mut labels = HashMap::new();
  labels.insert("io.nanocl".into(), "enabled".into());
  labels.insert("io.nanocl.c".into(), key);
  labels.insert("io.nanocl.n".into(), namespace.into());
  labels.insert("io.nanocl.cnsp".into(), namespace.into());

  let mut curr_label = cargo.container.labels.clone().unwrap_or_default();

  curr_label.extend(labels);

  CargoConfigPartial {
    container: ContainerConfig::<String> {
      labels: Some(curr_label),
      ..cargo.container.clone()
    },
    ..cargo.clone()
  }
}
