use nanocl_models::state::{StateDeployment, StateCargo, StateResources};

use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::Pool;

pub async fn deployment(
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

pub async fn cargo(
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

pub async fn resource(
  data: StateResources,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  for resource in data.resources {
    repositories::resource::create_or_patch(&resource, pool).await?;
  }
  Ok(())
}
