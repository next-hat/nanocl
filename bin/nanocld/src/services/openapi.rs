use ntex::web;
use ntex::http;
use ntex_files as fs;
use utoipa::Modify;
use utoipa::{OpenApi, ToSchema};
use bollard_next::service::{
  Driver, ConfigSpec, Config, HostConfig, NetworkingConfig,
  SwarmSpecCaConfigExternalCasProtocolEnum, ImageInspect, ImageSummary,
  TlsInfo, SwarmSpecCaConfig, SwarmSpecDispatcher, SwarmSpecEncryptionConfig,
  SwarmSpecOrchestration, SwarmSpecRaft, SwarmSpecTaskDefaults, ObjectVersion,
  SwarmSpec, SystemInfoCgroupDriverEnum, SystemInfoCgroupVersionEnum, Commit,
  IndexInfo, ClusterInfo, LocalNodeState, PeerNode,
  SystemInfoDefaultAddressPools, SystemInfoIsolationEnum, PluginsInfo,
  RegistryServiceConfig, Runtime, SwarmInfo, SystemInfo, EndpointIpamConfig,
  EndpointSettings, MountPointTypeEnum, PortTypeEnum,
  ContainerSummaryHostConfig, ContainerSummaryNetworkSettings, MountPoint,
  Port, ContainerSummary, HealthConfig, ContainerConfig, GraphDriverData,
  ImageInspectMetadata, ImageInspectRootFs, SwarmSpecCaConfigExternalCas,
  SwarmSpecTaskDefaultsLogDriver, GenericResources,
  GenericResourcesDiscreteResourceSpec, GenericResourcesNamedResourceSpec,
};
use nanocl_stubs::system::HostInfo;
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::node::{Node, NodeContainerSummary};
use nanocl_stubs::namespace::{
  Namespace, NamespaceSummary, NamespacePartial, NamespaceInspect,
};
use nanocl_stubs::cargo::{
  Cargo, CargoInspect, CargoSummary, CargoKillOptions, CreateExecOptions,
};
use nanocl_stubs::cargo_config::{
  CargoConfig, CargoConfigPartial, CargoConfigUpdate, ReplicationMode,
};
use nanocl_stubs::cargo_image::CargoImagePartial;
use nanocl_stubs::resource::{
  Resource, ResourcePatch, ResourceConfig, ResourcePartial,
};

use crate::error::HttpError;

use super::{node, cargo, system, namespace, cargo_image, resource};

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

    openapi.info.title = "Nanocl Daemon Endpoints".to_string();
    openapi.info.version = format!("v{}", env!("CARGO_PKG_VERSION"));
    openapi.servers = Some(vec![server]);
  }
}

/// Main structure to generate OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
  paths(
    // Node
    node::list_node,
    node::node_ws,
    // System
    system::get_info,
    system::watch_event,
    system::get_processes,
    // Namespace
    namespace::list_namespace,
    namespace::inspect_namespace,
    namespace::create_namespace,
    namespace::delete_namespace,
    // Cargo
    cargo::list_cargo,
    cargo::list_cargo_instance,
    cargo::inspect_cargo,
    cargo::create_cargo,
    cargo::delete_cargo,
    cargo::start_cargo,
    cargo::stop_cargo,
    cargo::put_cargo,
    cargo::patch_cargo,
    cargo::exec_command,
    cargo::kill_cargo,
    cargo::list_cargo_history,
    cargo::reset_cargo,
    cargo::logs_cargo,
    // Cargo Image
    cargo_image::list_cargo_image,
    cargo_image::inspect_cargo_image,
    cargo_image::create_cargo_image,
    cargo_image::delete_cargo_image,
    cargo_image::import_cargo_image,
    // Resource
    resource::list_resource,
    resource::inspect_resource,
    resource::create_resource,
    resource::delete_resource,
    resource::patch_resource,
    resource::list_resource_history,
    resource::reset_resource,
  ),
  components(schemas(
    // Node
    Node,
    NodeContainerSummary,
    // System
    HostInfo,
    SystemInfo,
    Commit,
    Runtime,
    SwarmInfo,
    PluginsInfo,
    GenericResources,
    RegistryServiceConfig,
    SystemInfoCgroupDriverEnum,
    SystemInfoDefaultAddressPools,
    SystemInfoCgroupVersionEnum,
    SystemInfoIsolationEnum,
    IndexInfo,
    ClusterInfo,
    LocalNodeState,
    PeerNode,
    SwarmSpec,
    ObjectVersion,
    TlsInfo,
    SwarmSpecCaConfig,
    SwarmSpecDispatcher,
    SwarmSpecEncryptionConfig,
    SwarmSpecOrchestration,
    SwarmSpecRaft,
    SwarmSpecTaskDefaults,
    SwarmSpecCaConfigExternalCas,
    SwarmSpecTaskDefaultsLogDriver,
    SwarmSpecCaConfigExternalCasProtocolEnum,
    GenericResourcesDiscreteResourceSpec,
    GenericResourcesNamedResourceSpec,
    // Namespace
    Namespace,
    NamespacePartial,
    NamespaceInspect,
    NamespaceSummary,
    // Cargo
    Cargo,
    CreateExecOptions,
    CargoKillOptions,
    CargoInspect,
    CargoConfig,
    ReplicationMode,
    CargoSummary,
    CargoConfigPartial,
    CargoConfigUpdate,
    // Container Image
    ImageSummary,
    ImageInspect,
    ImageInspectMetadata,
    ImageInspectRootFs,
    GraphDriverData,
    CargoImagePartial,
    // Container
    Config,
    Driver,
    NetworkingConfig,
    ConfigSpec,
    HostConfig,
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
    // Resource
    Resource,
    ResourcePatch,
    ResourceConfig,
    ResourcePartial,
    // Error
    ApiError,
    // Generic Response
    GenericDelete,
  )),
  tags(
    (name = "Cargo Images", description = "Cargo images management endpoints."),
    (name = "Namespaces", description = "Namespaces management endpoints."),
    (name = "Nodes", description = "Nodes management endpoints."),
    (name = "Resources", description = "Resources management endpoints."),
    (name = "System", description = "General system endpoints."),
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
