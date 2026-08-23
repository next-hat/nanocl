use ntex::web;

use nanocl_error::{http::HttpResult, io::IoError};
use nanocl_stubs::{cargo_spec::CargoSpec, generic::GenericNspQuery};

use crate::{
  models::{CargoDb, CargoObjCreateIn, SystemState},
  objects::generic::*,
  utils,
};

fn prepare_create_input(
  namespace: String,
  version: String,
  spec: CargoSpec,
) -> HttpResult<CargoObjCreateIn> {
  spec.validate().map_err(|error| {
    IoError::invalid_input("CargoSpec".to_owned(), error.to_string())
  })?;
  Ok(CargoObjCreateIn {
    namespace,
    spec,
    version,
  })
}

/// Create a new cargo by it specification
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  path = "/cargoes",
  request_body = CargoSpec,
  params(
    ("namespace" = Option<String>, Query, description = "Namespace where to create the cargo default to 'global'"),
  ),
  responses(
    (status = 201, description = "Cargo created", body = nanocl_stubs::cargo::Cargo),
    (status = 409, description = "Cargo already exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::post("/cargoes")]
pub async fn create_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<String>,
  payload: web::types::Json<CargoSpec>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let obj =
    prepare_create_input(namespace, path.into_inner(), payload.into_inner())?;
  let cargo = CargoDb::create_obj(&obj, &state).await?;
  Ok(web::HttpResponse::Created().json(&cargo))
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::{
    cargo_spec::{Config, ContainerSpec},
    generic::ImagePullPolicy,
  };

  use super::*;

  fn container(name: &str, image: &str) -> ContainerSpec {
    ContainerSpec {
      name: name.to_owned(),
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: ImagePullPolicy::IfNotPresent,
      container_config: Config {
        image: Some(image.to_owned()),
        ..Default::default()
      },
    }
  }

  #[test]
  fn prepares_final_single_container_declaration_for_object_creation() {
    let spec = CargoSpec {
      name: "single".to_owned(),
      containers: vec![container("main", "example/app:1")],
      ..Default::default()
    };

    let prepared = prepare_create_input(
      "global".to_owned(),
      "v0.18.0".to_owned(),
      spec.clone(),
    )
    .unwrap();

    assert_eq!(prepared.namespace, "global");
    assert_eq!(prepared.version, "v0.18.0");
    assert_eq!(prepared.spec, spec);
  }

  #[test]
  fn prepares_final_multi_container_declaration_for_object_creation() {
    let spec = CargoSpec {
      name: "multi".to_owned(),
      init_containers: vec![container("migrate", "example/migrate:1")],
      containers: vec![
        container("api", "example/api:1"),
        container("worker", "example/worker:1"),
      ],
      ..Default::default()
    };

    let prepared = prepare_create_input(
      "team".to_owned(),
      "v0.18.0".to_owned(),
      spec.clone(),
    )
    .unwrap();

    assert_eq!(prepared.namespace, "team");
    assert_eq!(prepared.version, "v0.18.0");
    assert_eq!(prepared.spec, spec);
  }

  #[test]
  fn rejects_invalid_complete_declaration_before_object_creation() {
    let result = prepare_create_input(
      "global".to_owned(),
      "v0.18.0".to_owned(),
      CargoSpec {
        name: "invalid".to_owned(),
        containers: Vec::new(),
        ..Default::default()
      },
    );
    let Err(error) = result else {
      panic!("an empty application-container list must be rejected");
    };

    assert_eq!(error.status, ntex::http::StatusCode::BAD_REQUEST);
    assert_eq!(
      error.msg,
      "CargoSpec: CargoSpec.Containers must contain at least one container"
    );
  }
}
