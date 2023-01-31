use ntex::http::StatusCode;

use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateResources, StateConfig,
};

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
  docker_api: &bollard::Docker,
  pool: &Pool,
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
      utils::cargo::create_or_patch(&namespace, &cargo, docker_api, pool)
        .await?;
      utils::cargo::start(
        &utils::key::gen_key(&namespace, &cargo.name),
        docker_api,
      )
      .await?;
    }
  }

  if let Some(resources) = data.resources {
    for resource in resources {
      repositories::resource::create_or_patch(&resource, pool).await?;
    }
  }

  Ok(())
}

pub async fn apply_cargo(
  data: StateCargo,
  docker_api: &bollard::Docker,
  pool: &Pool,
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
    utils::cargo::create_or_patch(&namespace, &cargo, docker_api, pool).await?;
    utils::cargo::start(
      &utils::key::gen_key(&namespace, &cargo.name),
      docker_api,
    )
    .await?;
  }

  Ok(())
}

pub async fn apply_resource(
  data: StateResources,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  for resource in data.resources {
    repositories::resource::create_or_patch(&resource, pool).await?;
  }
  Ok(())
}

pub async fn revert_deployment(
  data: StateDeployment,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  let namespace = if let Some(namespace) = data.namespace {
    namespace
  } else {
    "global".into()
  };

  if let Some(cargoes) = data.cargoes {
    for cargo in cargoes {
      let key = utils::key::gen_key(&namespace, &cargo.name);
      utils::cargo::delete(&key, docker_api, pool, Some(true)).await?;
    }
  }

  if let Some(resources) = data.resources {
    for resource in resources {
      repositories::resource::delete_by_key(resource.name, pool).await?;
    }
  }

  if namespace != "global" {
    utils::namespace::delete_by_name(&namespace, docker_api, pool).await?;
  }

  Ok(())
}

pub async fn revert_cargo(
  data: StateCargo,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  let namespace = if let Some(namespace) = data.namespace {
    namespace
  } else {
    "global".into()
  };

  for cargo in data.cargoes {
    let key = utils::key::gen_key(&namespace, &cargo.name);
    utils::cargo::delete(&key, docker_api, pool, Some(true)).await?;
  }

  if namespace != "global" {
    utils::namespace::delete_by_name(&namespace, docker_api, pool).await?;
  }

  Ok(())
}

pub async fn revert_resource(
  data: StateResources,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  for resource in data.resources {
    repositories::resource::delete_by_key(resource.name, pool).await?;
  }
  Ok(())
}
