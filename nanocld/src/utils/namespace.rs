use std::collections::HashMap;

use bollard::container::ListContainersOptions;
use bollard::service::ContainerSummary;
use nanocl_models::namespace::NamespaceSummary;
use ntex::http::StatusCode;

use crate::repositories;
use crate::models::Pool;
use crate::error::HttpResponseError;

/// List containers based on the namespace
///
/// ## Arguments
/// - [namespace](String) - The namespace
/// - [docker_api](bollard::Docker) - The docker api
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers have been listed
///   - [Err](HttpResponseError) - The containers have not been listed
///
pub async fn list_instance(
  namespace: &str,
  docker_api: &bollard::Docker,
) -> Result<Vec<ContainerSummary>, HttpResponseError> {
  let label = format!("namespace={}", namespace);
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
