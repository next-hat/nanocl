use ntex::util::HashMap;
use ntex::web;
use ntex::http;
use ntex_files as fs;
use serde::{Serialize, Deserialize};
use utoipa::{OpenApi, Modify, ToSchema};
use bollard_next::container::Config;
use bollard_next::service::{
  PortBinding, MountBindOptionsPropagationEnum, MountVolumeOptionsDriverConfig,
  MountBindOptions, MountTmpfsOptions, MountTypeEnum, MountVolumeOptions,
  RestartPolicyNameEnum, ThrottleDevice, ResourcesBlkioWeightDevice,
  HostConfigCgroupnsModeEnum, DeviceRequest, DeviceMapping,
  HostConfigIsolationEnum, HostConfigLogConfig, Mount, RestartPolicy,
  ResourcesUlimits, Driver, ConfigSpec, HostConfig, NetworkingConfig,
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
use nanocl_stubs::vm_image::{VmImage, VmImageResizePayload};
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
use nanocl_stubs::vm::{Vm, VmInspect, VmSummary};
use nanocl_stubs::vm_config::{
  VmConfig, VmConfigPartial, VmConfigUpdate, VmDiskConfig, VmHostConfig,
};
use nanocl_stubs::resource::{
  Resource, ResourcePatch, ResourceConfig, ResourcePartial,
};

use crate::error::HttpError;

use super::{node, system, namespace, cargo, cargo_image, vm, vm_image, resource};

/// When returning a [HttpError](HttpError) the status code is stripped and the error is returned as a json object with the message field set to the error message.
#[allow(dead_code)]
#[derive(ToSchema)]
struct ApiError {
  msg: String,
}

/// Helper to generate have Any type for [OpenApi](OpenApi) usefull for dynamic json objects like [ResourceConfig](ResourceConfig)
#[allow(dead_code)]
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
enum Any {
  String(String),
  Number(f64),
  Bool(bool),
  Array(Vec<Any>),
  Object(HashMap<String, Any>),
}

struct PortMap;

impl<'__s> utoipa::ToSchema<'__s> for PortMap {
  fn schema() -> (
    &'__s str,
    utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
  ) {
    (
      "PortMap",
      utoipa::openapi::ObjectBuilder::new()
        .nullable(true)
        .title(Some("PortMap"))
        .description(Some("PortMap"))
        .schema_type(utoipa::openapi::schema::SchemaType::Object)
        .property(
          "<port/tcp|udp>",
          utoipa::openapi::ArrayBuilder::new()
            .items(
              utoipa::openapi::ObjectBuilder::new()
                .property(
                  "HostPort",
                  utoipa::openapi::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::SchemaType::String)
                    .build(),
                )
                .property(
                  "HostIp",
                  utoipa::openapi::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::SchemaType::String)
                    .build(),
                )
                .build(),
            )
            .build(),
        )
        .into(),
    )
  }
}

/// Helper to generate the versioned OpenAPI documentation
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
      .parameter("version", variable)
      .build();

    openapi.info.title = "Nanocl Daemon".to_string();
    openapi.info.version = format!("v{}", env!("CARGO_PKG_VERSION"));
    openapi.info.description =
      Some(include_str!("../../specs/readme.md").to_string());
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
    // VM Image
    vm_image::list_vm_images,
    vm_image::import_vm_image,
    vm_image::delete_vm_image,
    vm_image::resize_vm_image,
    vm_image::clone_vm_image,
    vm_image::snapshot_vm_image,
    // Vm
    vm::list_vm,
    vm::inspect_vm,
    vm::start_vm,
    vm::stop_vm,
    vm::delete_vm,
    vm::create_vm,
    vm::list_vm_history,
    vm::patch_vm,
    vm::vm_attach,
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
    PortMap,
    PortBinding,
    MountPoint,
    MountPointTypeEnum,
    EndpointSettings,
    PortTypeEnum,
    EndpointIpamConfig,
    ThrottleDevice,
    ResourcesBlkioWeightDevice, HostConfigCgroupnsModeEnum,
    DeviceRequest,
    DeviceMapping,
    HostConfigIsolationEnum,
    HostConfigLogConfig,
    Mount,
    RestartPolicy,
    ResourcesUlimits,
    MountBindOptions,
    MountTmpfsOptions,
    MountTypeEnum,
    MountVolumeOptions,
    RestartPolicyNameEnum,
    MountBindOptionsPropagationEnum,
    MountVolumeOptionsDriverConfig,
    // Vm Image
    VmImage,
    VmImageResizePayload,
    // Vm
    Vm,
    VmSummary,
    VmInspect,
    // Vm Config
    VmConfig,
    VmConfigPartial,
    VmConfigUpdate,
    VmDiskConfig,
    VmHostConfig,
    // Resource
    Resource,
    ResourcePatch,
    ResourceConfig,
    ResourcePartial,
    // Error
    ApiError,
    // Generic Response
    Any,
    GenericDelete,
  )),
  tags(
    (name = "CargoImages", description = "Cargo images management endpoints."),
    (name = "Namespaces", description = "Namespaces management endpoints."),
    (name = "Nodes", description = "Nodes management endpoints."),
    (name = "Resources", description = "Resources management endpoints."),
    (name = "System", description = "General system endpoints."),
    (name = "VmImages", description = "Virtual machine images management endpoints."),
    (name = "Vms", description = "Virtual machines management endpoints."),
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
  let yaml = ApiDoc::openapi()
    .to_yaml()
    .expect("Unable to generate openapi spec");

  std::fs::write("./bin/nanocld/specs/swagger.yaml", yaml).unwrap();

  config.service(get_api_specs);
  config.service(
    fs::Files::new("/", "./bin/nanocld/swagger-ui/").index_file("index.html"),
  );
}
