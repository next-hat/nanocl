use ntex::web;
use ntex::http;
use ntex_files as fs;
use utoipa::Modify;
use utoipa::{OpenApi, ToSchema};
use bollard_next::models::{ImageInspect, ImageSummary};
use bollard_next::service::{
  EndpointIpamConfig, EndpointSettings, MountPointTypeEnum, PortTypeEnum,
  ContainerSummaryHostConfig, ContainerSummaryNetworkSettings, MountPoint,
  Port, ContainerSummary, HealthConfig, ContainerConfig, GraphDriverData,
  ImageInspectMetadata, ImageInspectRootFs,
};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::namespace::{
  Namespace, NamespaceSummary, NamespacePartial, NamespaceInspect,
};
use nanocl_stubs::cargo::CargoInspect;
use nanocl_stubs::cargo_config::{CargoConfig, ReplicationMode};
use nanocl_stubs::cargo_image::CargoImagePartial;

use crate::error::HttpError;

use super::{node, namespace, cargo_image};

/// Api error response
#[allow(dead_code)]
#[derive(ToSchema)]
struct ApiError {
  msg: String,
}

struct VersionModifier;

impl Modify for VersionModifier {
  fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
    let variable = utoipa::openapi::ServerVariableBuilder::default()
      .default_value("v0.4")
      .description(Some("API version"))
      .enum_values(Some(vec!["v0.4", "v0.3", "v0.2", "v0.1"]))
      .build();

    let server = utoipa::openapi::ServerBuilder::default()
      .url("/{version}")
      .description(Some("Local development server"))
      .parameter("version", variable)
      .build();

    openapi.info.title = "Nanocld API".to_string();
    openapi.info.version = "v0.4".to_string();
    openapi.servers = Some(vec![server]);
  }
}

/// Main structure to generate OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
  paths(
    // Cargo Image
    cargo_image::list_cargo_image,
    cargo_image::inspect_cargo_image,
    cargo_image::create_cargo_image,
    cargo_image::delete_cargo_image,
    cargo_image::import_cargo_image,
    // Namespace
    namespace::list_namespace,
    namespace::create_namespace,
    namespace::delete_namespace,
    namespace::inspect_namespace,
    // Node
    node::list_node,
    node::node_ws,
  ),
  components(schemas(
    // Namespace
    Namespace,
    NamespacePartial,
    NamespaceInspect,
    NamespaceSummary,
    // Node
    NodeContainerSummary,
    // Container Image
    ImageSummary,
    ImageInspect,
    ImageInspectMetadata,
    ImageInspectRootFs,
    GraphDriverData,
    CargoImagePartial,
    // Container
    ContainerConfig,
    HealthConfig,
    ContainerSummary,
    ContainerSummaryHostConfig,
    ContainerSummaryNetworkSettings,
    Port,
    MountPoint,
    MountPointTypeEnum,
    EndpointSettings,
    PortTypeEnum,
    EndpointIpamConfig,
    // Cargo
    CargoInspect,
    CargoConfig,
    ReplicationMode,
    // Error
    ApiError,
    // Generic Response
    GenericDelete,
  )),
  tags(
    (name = "Cargo Images", description = "Cargo images management endpoints."),
    (name = "Namespaces", description = "Namespaces management endpoints."),
  ),
  modifiers(&VersionModifier),
)]
struct ApiDoc;

#[web::get("/swagger.json")]
async fn get_api_specs() -> Result<web::HttpResponse, HttpError> {
  let spec = ApiDoc::openapi().to_json().map_err(|err| HttpError {
    status: http::StatusCode::INTERNAL_SERVER_ERROR,
    msg: format!("Error generating OpenAPI spec: {}", err),
  })?;
  return Ok(
    web::HttpResponse::Ok()
      .content_type("application/json")
      .body(spec),
  );
}

pub fn ntex_config(config: &mut ntex::web::ServiceConfig) {
  config.service(get_api_specs);
  config.service(
    fs::Files::new("/", "./bin/nanocld/swagger-ui/").index_file("index.html"),
  );
}
