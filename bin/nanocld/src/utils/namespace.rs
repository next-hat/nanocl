use std::collections::HashMap;

use ntex::http::StatusCode;
use bollard::models::ContainerSummary;
use bollard::container::ListContainersOptions;

use nanocl_models::namespace::{NamespaceSummary, NamespaceInspect};

use crate::repositories;
use crate::models::Pool;
use crate::error::HttpResponseError;

use super::cargo;

/// ## List existing container in a namespace
///
/// List containers based on the namespace
///
/// ## Arguments
///
/// - [namespace](String) - The namespace
/// - [docker_api](bollard::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers have been listed
///   - [Err](HttpResponseError) - The containers have not been listed
///
/// ## Example
///
/// ```rust,norun
/// use bollard::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::list_instance("my-namespace", &docker_api).await;
/// ```
///
pub async fn list_instance(
  namespace: &str,
  docker_api: &bollard::Docker,
) -> Result<Vec<ContainerSummary>, HttpResponseError> {
  let label = format!("io.nanocl.namespace={}", namespace);
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

/// ## List namespaces
///
/// List all existing namespaces
///
/// ## Arguments
///
/// - [docker_api](bollard::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<NamespaceSummary>) - The namespaces have been listed
///   - [Err](HttpResponseError) - The namespaces have not been listed
///
/// ## Example
///
/// ```rust,norun
/// use bollard::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::list(&docker_api, &pool).await;
/// ```
///
pub async fn list(
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<Vec<NamespaceSummary>, HttpResponseError> {
  let items = repositories::namespace::list(pool).await?;
  let mut new_items = Vec::new();
  for item in items {
    let cargo_count =
      repositories::cargo::count_by_namespace(item.name.to_owned(), pool)
        .await?;
    let instance_count = list_instance(&item.name, docker_api).await?.len();
    new_items.push(NamespaceSummary {
      name: item.name.to_owned(),
      cargoes: cargo_count,
      instances: instance_count.try_into().unwrap(),
    })
  }
  Ok(new_items)
}

/// ## Inspect a namespace
///
/// Get detailed information about a namespace
///
/// ## Arguments
///
/// - [namespace](String) - The namespace
/// - [docker_api](bollard::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NamespaceInspect) - The namespace has been inspected
///   - [Err](HttpResponseError) - The namespace has not been inspected
///
/// ## Example
///
/// ```rust,norun
/// use bollard::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::inspect("my-namespace", &docker_api, &pool).await;
/// ```
///
pub async fn inspect(
  namespace: &str,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<NamespaceInspect, HttpResponseError> {
  let namespace =
    repositories::namespace::find_by_name(namespace.to_owned(), pool).await?;
  log::debug!("Found namespace to inspect {:?}", &namespace);
  let cargo_db_models =
    repositories::cargo::find_by_namespace(namespace.to_owned(), pool).await?;
  log::debug!("Found namespace cargoes to inspect {:?}", &cargo_db_models);
  let mut cargoes = Vec::new();
  for cargo in cargo_db_models {
    let cargo = cargo::inspect(&cargo.key, docker_api, pool).await?;
    cargoes.push(cargo);
  }
  Ok(NamespaceInspect {
    name: namespace.name,
    cargoes,
  })
}
