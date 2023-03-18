use ntex::rt;
use ntex::http::StatusCode;

use nanocl_stubs::system::Event;
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateResources, StateConfig,
};

use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::{StateData, DaemonState};

pub fn parse_state(
  data: &serde_json::Value,
) -> Result<StateData, HttpResponseError> {
  let meta =
    serde_json::from_value::<StateConfig>(data.to_owned()).map_err(|err| {
      HttpResponseError {
        status: StatusCode::BAD_REQUEST,
        msg: format!("unable to serialize payload {err}"),
      }
    })?;
  match meta.r#type.as_str() {
    "Deployment" => {
      let data = serde_json::from_value::<StateDeployment>(data.to_owned())
        .map_err(|err| HttpResponseError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Deployment(data))
    }
    "Cargo" => {
      let data = serde_json::from_value::<StateCargo>(data.to_owned())
        .map_err(|err| HttpResponseError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Cargo(data))
    }
    "Resource" => {
      let data = serde_json::from_value::<StateResources>(data.to_owned())
        .map_err(|err| HttpResponseError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Resource(data))
    }
    _ => Err(HttpResponseError {
      status: StatusCode::BAD_REQUEST,
      msg: "unknown type".into(),
    }),
  }
}

pub async fn apply_deployment(
  data: &StateDeployment,
  version: &str,
  state: &DaemonState,
) -> Result<(), HttpResponseError> {
  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(
      namespace,
      &state.docker_api,
      &state.pool,
    )
    .await?;
    namespace.to_owned()
  } else {
    "global".into()
  };

  if let Some(cargoes) = &data.cargoes {
    for cargo in cargoes {
      utils::cargo::create_or_put(
        &namespace,
        cargo,
        version,
        &state.docker_api,
        &state.pool,
      )
      .await?;
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
        state_ptr
          .event_emitter
          .lock()
          .unwrap()
          .send(Event::CargoPatched(Box::new(cargo)));
      });
      utils::cargo::start(
        &utils::key::gen_key(&namespace, &cargo.name),
        &state.docker_api,
        &state.pool,
      )
      .await?;
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
        state_ptr
          .event_emitter
          .lock()
          .unwrap()
          .send(Event::CargoStarted(Box::new(cargo)));
      });
    }
  }

  if let Some(resources) = &data.resources {
    for resource in resources {
      let key = resource.name.to_owned();
      utils::resource::create_or_patch(resource.clone(), &state.pool).await?;
      let state_ptr = state.clone();
      rt::spawn(async move {
        let item = repositories::resource::inspect_by_key(key, &state_ptr.pool)
          .await
          .unwrap();
        state_ptr
          .event_emitter
          .lock()
          .unwrap()
          .send(Event::ResourcePatched(Box::new(item)));
      });
    }
  }

  Ok(())
}

pub async fn apply_cargo(
  data: &StateCargo,
  version: &str,
  state: &DaemonState,
) -> Result<(), HttpResponseError> {
  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(
      namespace,
      &state.docker_api,
      &state.pool,
    )
    .await?;
    namespace.to_owned()
  } else {
    "global".into()
  };

  for cargo in &data.cargoes {
    utils::cargo::create_or_put(
      &namespace,
      cargo,
      version,
      &state.docker_api,
      &state.pool,
    )
    .await?;
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let state_ptr = state.clone();
    rt::spawn(async move {
      let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
      state_ptr
        .event_emitter
        .lock()
        .unwrap()
        .send(Event::CargoPatched(Box::new(cargo)));
    });
    utils::cargo::start(
      &utils::key::gen_key(&namespace, &cargo.name),
      &state.docker_api,
      &state.pool,
    )
    .await?;
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let state_ptr = state.clone();
    rt::spawn(async move {
      let cargo = utils::cargo::inspect(&key, &state_ptr).await.unwrap();
      state_ptr
        .event_emitter
        .lock()
        .unwrap()
        .send(Event::CargoStarted(Box::new(cargo)));
    });
  }

  Ok(())
}

pub async fn apply_resource(
  data: &StateResources,
  state: &DaemonState,
) -> Result<(), HttpResponseError> {
  for resource in &data.resources {
    let key = resource.name.to_owned();
    utils::resource::create_or_patch(resource.clone(), &state.pool).await?;
    let pool = state.pool.clone();
    let event_emitter = state.event_emitter.clone();
    rt::spawn(async move {
      let resource = repositories::resource::inspect_by_key(key, &pool)
        .await
        .unwrap();
      event_emitter
        .lock()
        .unwrap()
        .send(Event::ResourcePatched(Box::new(resource)));
    });
  }
  Ok(())
}

pub async fn revert_deployment(
  data: &StateDeployment,
  state: &DaemonState,
) -> Result<(), HttpResponseError> {
  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };

  if let Some(cargoes) = &data.cargoes {
    for cargo in cargoes {
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let cargo = utils::cargo::inspect(&key, state).await?;
      utils::cargo::delete(&key, &state.docker_api, &state.pool, Some(true))
        .await?;
      let state_ptr = state.clone();
      rt::spawn(async move {
        state_ptr
          .event_emitter
          .lock()
          .unwrap()
          .send(Event::CargoDeleted(Box::new(cargo)));
      });
    }
  }

  if let Some(resources) = &data.resources {
    for resource in resources {
      let key = resource.name.to_owned();
      let resource =
        repositories::resource::inspect_by_key(key, &state.pool).await?;
      utils::resource::delete(resource.clone(), &state.pool).await?;
      let state_ptr = state.clone();
      rt::spawn(async move {
        state_ptr
          .event_emitter
          .lock()
          .unwrap()
          .send(Event::ResourceDeleted(Box::new(resource)));
      });
    }
  }

  Ok(())
}

pub async fn revert_cargo(
  data: &StateCargo,
  state: &DaemonState,
) -> Result<(), HttpResponseError> {
  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };

  for cargo in &data.cargoes {
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let cargo = utils::cargo::inspect(&key, state).await?;
    utils::cargo::delete(&key, &state.docker_api, &state.pool, Some(true))
      .await?;
    let event_emitter = state.event_emitter.clone();
    rt::spawn(async move {
      event_emitter
        .lock()
        .unwrap()
        .send(Event::CargoDeleted(Box::new(cargo)));
    });
  }

  Ok(())
}

pub async fn revert_resource(
  data: &StateResources,
  state: &DaemonState,
) -> Result<(), HttpResponseError> {
  for resource in &data.resources {
    let key = resource.name.to_owned();
    let resource =
      repositories::resource::inspect_by_key(key, &state.pool).await?;
    utils::resource::delete(resource.clone(), &state.pool).await?;
    let event_emitter = state.event_emitter.clone();
    rt::spawn(async move {
      event_emitter
        .lock()
        .unwrap()
        .send(Event::ResourceDeleted(Box::new(resource)));
    });
  }
  Ok(())
}
