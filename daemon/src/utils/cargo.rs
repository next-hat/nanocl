use std::collections::HashMap;

use ntex::http::StatusCode;
use bollard::service::ContainerSummary;
use bollard::container::{ListContainersOptions, RemoveContainerOptions};

use nanocl_models::cargo::{Cargo, CargoPartial, CargoSummary};

use crate::repositories;
use crate::error::HttpResponseError;
use crate::models::Pool;

/// Create containers based on the cargo config
/// The number of containers created is based on the number of instances
/// defined in the cargo config
/// If the number of instances is greater than 1, the containers will be named
/// with the cargo key and a number
/// Example: cargo-key-1, cargo-key-2, cargo-key-3
/// If the number of instances is equal to 1, the container will be named with
/// the cargo key
/// Example: cargo-key
/// The cargo key is used to track the containers
///
/// ## Arguments
/// - [cargo](Cargo) - The cargo
/// - [number](i64) - The number of containers to create
/// - [docker_api](bollard::Docker) - The docker api
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Ok) - The containers has been created
///   - [Err](HttpResponseError) - The containers has not been created
///
async fn create_instance(
  cargo: &Cargo,
  number: i64,
  docker_api: &bollard::Docker,
) -> Result<(), HttpResponseError> {
  for current in 0..number {
    let name = if current > 0 {
      format!("{}-{}", cargo.key, current)
    } else {
      cargo.key.to_owned()
    };

    let create_options = bollard::container::CreateContainerOptions {
      name,
      ..Default::default()
    };

    // Add cargo label to the container to track it
    let mut labels =
      cargo.config.container.labels.to_owned().unwrap_or_default();
    labels.insert("cargo".to_string(), cargo.key.to_owned());

    let config = bollard::container::Config {
      labels: Some(labels),
      ..cargo.config.container.to_owned()
    };

    docker_api
      .create_container::<String, String>(Some(create_options), config)
      .await
      .map_err(|e| HttpResponseError {
        msg: format!("Unable to create container: {}", e),
        status: StatusCode::BAD_REQUEST,
      })?;
  }

  Ok(())
}

/// List containers based on the cargo config
///
/// ## Arguments
/// - [cargo_key](String) - The cargo key
/// - [docker_api](bollard::Docker) - The docker api
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers has been listed
///   - [Err](HttpResponseError) - The containers has not been listed
///
pub async fn list_instance(
  cargo_key: &str,
  docker_api: &bollard::Docker,
) -> Result<Vec<ContainerSummary>, HttpResponseError> {
  let label = format!("cargo={}", cargo_key);
  let mut filters: HashMap<&str, Vec<&str>> = HashMap::new();
  filters.insert("label", vec![&label]);
  let options = Some(ListContainersOptions {
    all: true,
    filters,
    ..Default::default()
  });
  let containers = docker_api.list_containers(options).await.map_err(|e| {
    HttpResponseError {
      msg: format!("Unable to list containers got error : {}", e),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    }
  })?;

  Ok(containers)
}

/// Create a new cargo with his containers
///
/// ## Arguments
/// - [cargo_partial](CargoPartial) - The cargo partial
/// - [namespace](String) - The namespace
/// - [docker_api](bollard::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo has been created
///   - [Err](HttpResponseError) - The cargo has not been created
///
pub async fn create(
  cargo_partial: CargoPartial,
  namespace: String,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<Cargo, HttpResponseError> {
  let cargo =
    repositories::cargo::create(namespace, cargo_partial.to_owned(), pool)
      .await?;

  if let Err(err) = create_instance(&cargo, 1, docker_api).await {
    repositories::cargo::delete_by_key(cargo.key.to_owned(), pool).await?;
    return Err(err);
  }

  Ok(cargo)
}

/// Start containers of the given cargo
/// The containers are started in parallel
/// If one container fails to start, the other containers will continue to start
///
/// ## Arguments
/// - [cargo_key](str) - The cargo key
/// - [docker_api](bollard::Docker) - The docker api
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been started
///   - [Err](HttpResponseError) - The containers has not been started
///
pub async fn start(
  cargo_key: &str,
  docker_api: &bollard::Docker,
) -> Result<(), HttpResponseError> {
  let containers = list_instance(cargo_key, docker_api).await?;

  for container in containers {
    docker_api
      .start_container::<String>(&container.id.unwrap_or_default(), None)
      .await
      .map_err(|e| HttpResponseError {
        msg: format!("Unable to start container got error : {}", e),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  Ok(())
}

/// Stop containers of the given cargo
///
/// ## Arguments
/// - [cargo_key](str) - The cargo key
/// - [docker_api](bollard::Docker) - The docker api
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///  - [Ok](Ok) - The containers has been stopped
///  - [Err](HttpResponseError) - The containers has not been stopped
///
/// ## Example
/// ```rust,norun
/// stop("global-test", &docker_api).await.expect("Unable to stop cargo);
/// ```
///
pub async fn stop(
  cargo_key: &str,
  docker_api: &bollard::Docker,
) -> Result<(), HttpResponseError> {
  let containers = list_instance(cargo_key, docker_api).await?;

  for container in containers {
    docker_api
      .stop_container(&container.id.unwrap_or_default(), None)
      .await
      .map_err(|e| HttpResponseError {
        msg: format!("Unable to stop container got error : {}", e),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  Ok(())
}

/// Delete containers of the given cargo and the cargo itself
///
/// ## Arguments
/// - [cargo_key](str) - The cargo key
/// - [docker_api](bollard::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Ok) - The cargo has been deleted
///   - [Err](HttpResponseError) - The cargo has not been deleted
///
/// ## Example
/// ```rust,norun
/// delete("global-test", &docker_api, &pool).await.expect("Unable to delete cargo);
/// ```
///
pub async fn delete(
  cargo_key: &str,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  let containers = list_instance(cargo_key, docker_api).await?;

  println!("containers : {:?}", &containers);

  for container in containers {
    docker_api
      .remove_container(
        &container.id.unwrap_or_default(),
        None::<RemoveContainerOptions>,
      )
      .await
      .map_err(|e| HttpResponseError {
        msg: format!("Unable to remove container got error : {}", e),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  repositories::cargo::delete_by_key(cargo_key.to_owned(), pool).await?;

  Ok(())
}

/// Patch a cargo
/// The cargo is patched and the containers are updated
/// The containers are updated in parallel
/// If one container fails to update, the other containers will continue to update
/// The containers are updated with the new cargo configuration
///
/// ## Arguments
/// - [cargo_key](str) - The cargo key
/// - [cargo_partial](CargoPartial) - The cargo partial
/// - [docker_api](bollard::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo has been patched
///   - [Err](HttpResponseError) - The cargo has not been patched
///
pub async fn patch(
  cargo_key: &str,
  cargo_partial: CargoPartial,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<Cargo, HttpResponseError> {
  let cargo = repositories::cargo::update_by_key(
    cargo_key.to_owned(),
    cargo_partial,
    pool,
  )
  .await?;

  let containers = list_instance(cargo_key, docker_api).await?;

  // Rename existing container to avoid name conflict
  for container in containers.iter().cloned() {
    let names = container.names.unwrap_or_default();
    let name = format!("{}-backup", names[0]);

    docker_api
      .rename_container(
        &container.id.unwrap_or_default(),
        bollard::container::RenameContainerOptions { name },
      )
      .await
      .map_err(|e| HttpResponseError {
        msg: format!("Unable to rename container got error : {}", e),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  // Create instance with the new config
  create_instance(&cargo, 1, docker_api).await?;

  // Delete old containers
  for container in containers {
    docker_api
      .remove_container(
        &container.id.unwrap_or_default(),
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
      )
      .await
      .map_err(|e| HttpResponseError {
        msg: format!("Unable to remove container got error : {}", e),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  Ok(cargo)
}

/// List cargo in given namespace
/// The containers are filtered by the cargo key
///
/// ## Arguments
/// - [nsp](str) - The namespace name
/// - [docker_api](bollard::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers of the cargo
///   - [Err](HttpResponseError) - The containers of the cargo has not been listed
///
pub async fn list(
  nsp: &str,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<Vec<CargoSummary>, HttpResponseError> {
  let namespace =
    repositories::namespace::find_by_name(nsp.to_owned(), pool).await?;

  let cargoes = repositories::cargo::find_by_namespace(namespace, pool).await?;

  let mut cargo_summaries = Vec::new();

  for cargo in cargoes {
    let config =
      repositories::cargo_config::find_by_key(cargo.config_key, pool).await?;
    let containers = list_instance(&cargo.key, docker_api).await?;

    let mut running_instances = 0;
    for container in containers {
      if container.state == Some("running".into()) {
        running_instances += 1;
      }
    }

    cargo_summaries.push(CargoSummary {
      key: cargo.key,
      name: cargo.name,
      namespace_name: cargo.namespace_name,
      config: config.to_owned(),
      running_instances,
      config_key: config.key,
    });
  }

  Ok(cargo_summaries)
}
