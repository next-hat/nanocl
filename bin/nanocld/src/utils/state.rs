use ntex::rt;
use ntex::http::StatusCode;

use nanocl_stubs::system::Event;
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateResources, StateConfig,
};

use crate::event::EventEmitterPtr;
use crate::repositories::resource;
use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::{Pool, StateData};

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
  data: StateDeployment,
  docker_api: &bollard_next::Docker,
  pool: &Pool,
  event_emitter: &EventEmitterPtr,
) -> Result<(), HttpResponseError> {
  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = data.namespace {
    utils::namespace::create_if_not_exists(&namespace, docker_api, pool)
      .await?;
    namespace
  } else {
    "global".into()
  };

  if let Some(cargoes) = data.cargoes {
    for cargo in cargoes {
      utils::cargo::create_or_put(&namespace, &cargo, docker_api, pool).await?;
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let p = pool.clone();
      let ev = event_emitter.clone();
      let docker = docker_api.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &docker, &p).await.unwrap();
        ev.lock()
          .unwrap()
          .send(Event::CargoPatched(Box::new(cargo)));
      });
      utils::cargo::start(
        &utils::key::gen_key(&namespace, &cargo.name),
        docker_api,
      )
      .await?;
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let p = pool.clone();
      let ev = event_emitter.clone();
      let docker = docker_api.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect(&key, &docker, &p).await.unwrap();
        ev.lock()
          .unwrap()
          .send(Event::CargoStarted(Box::new(cargo)));
      });
    }
  }

  if let Some(resources) = data.resources {
    for resource in resources {
      let key = resource.name.to_owned();
      repositories::resource::create_or_patch(&resource, pool).await?;
      let p = pool.clone();
      let ev = event_emitter.clone();
      rt::spawn(async move {
        let item = resource::inspect_by_key(key, &p).await.unwrap();
        ev.lock()
          .unwrap()
          .send(Event::ResourcePatched(Box::new(item)));
      });
    }
  }

  Ok(())
}

pub async fn apply_cargo(
  data: StateCargo,
  docker_api: &bollard_next::Docker,
  pool: &Pool,
  event_emitter: &EventEmitterPtr,
) -> Result<(), HttpResponseError> {
  // If we have a namespace and it doesn't exist, create it
  // Unless we use `global` as default for the creation of cargoes
  let namespace = if let Some(namespace) = data.namespace {
    utils::namespace::create_if_not_exists(&namespace, docker_api, pool)
      .await?;
    namespace
  } else {
    "global".into()
  };

  for cargo in data.cargoes {
    utils::cargo::create_or_put(&namespace, &cargo, docker_api, pool).await?;
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let p = pool.clone();
    let ev = event_emitter.clone();
    let docker = docker_api.clone();
    rt::spawn(async move {
      let cargo = utils::cargo::inspect(&key, &docker, &p).await.unwrap();
      ev.lock()
        .unwrap()
        .send(Event::CargoPatched(Box::new(cargo)));
    });
    utils::cargo::start(
      &utils::key::gen_key(&namespace, &cargo.name),
      docker_api,
    )
    .await?;
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let p = pool.clone();
    let ev = event_emitter.clone();
    let docker = docker_api.clone();
    rt::spawn(async move {
      let cargo = utils::cargo::inspect(&key, &docker, &p).await.unwrap();
      ev.lock()
        .unwrap()
        .send(Event::CargoStarted(Box::new(cargo)));
    });
  }

  Ok(())
}

pub async fn apply_resource(
  data: StateResources,
  pool: &Pool,
  event_emitter: &EventEmitterPtr,
) -> Result<(), HttpResponseError> {
  for resource in data.resources {
    let key = resource.name.to_owned();
    repositories::resource::create_or_patch(&resource, pool).await?;
    let pool = pool.clone();
    let event_emitter = event_emitter.clone();
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
  data: StateDeployment,
  docker_api: &bollard_next::Docker,
  pool: &Pool,
  event_emitter: &EventEmitterPtr,
) -> Result<(), HttpResponseError> {
  let namespace = if let Some(namespace) = data.namespace {
    namespace
  } else {
    "global".into()
  };

  if let Some(cargoes) = data.cargoes {
    for cargo in cargoes {
      let key = utils::key::gen_key(&namespace, &cargo.name);
      let cargo = utils::cargo::inspect(&key, docker_api, pool).await?;
      utils::cargo::delete(&key, docker_api, pool, Some(true)).await?;
      let event_emitter = event_emitter.clone();
      rt::spawn(async move {
        event_emitter
          .lock()
          .unwrap()
          .send(Event::CargoDeleted(Box::new(cargo)));
      });
    }
  }

  if let Some(resources) = data.resources {
    for resource in resources {
      let key = resource.name.to_owned();
      let resource = repositories::resource::inspect_by_key(key, pool).await?;
      repositories::resource::delete_by_key(resource.name.to_owned(), pool)
        .await?;
      let event_emitter = event_emitter.clone();
      rt::spawn(async move {
        event_emitter
          .lock()
          .unwrap()
          .send(Event::ResourceDeleted(Box::new(resource)));
      });
    }
  }

  Ok(())
}

pub async fn revert_cargo(
  data: StateCargo,
  docker_api: &bollard_next::Docker,
  pool: &Pool,
  event_emitter: &EventEmitterPtr,
) -> Result<(), HttpResponseError> {
  let namespace = if let Some(namespace) = data.namespace {
    namespace
  } else {
    "global".into()
  };

  for cargo in data.cargoes {
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let cargo = utils::cargo::inspect(&key, docker_api, pool).await?;
    utils::cargo::delete(&key, docker_api, pool, Some(true)).await?;
    let event_emitter = event_emitter.clone();
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
  data: StateResources,
  pool: &Pool,
  event_emitter: &EventEmitterPtr,
) -> Result<(), HttpResponseError> {
  for resource in data.resources {
    let key = resource.name.to_owned();
    let resource = repositories::resource::inspect_by_key(key, pool).await?;
    repositories::resource::delete_by_key(resource.name.to_owned(), pool)
      .await?;
    let event_emitter = event_emitter.clone();
    rt::spawn(async move {
      event_emitter
        .lock()
        .unwrap()
        .send(Event::ResourceDeleted(Box::new(resource)));
    });
  }
  Ok(())
}
