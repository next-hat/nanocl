use ntex::util::HashMap;
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
  SwarmSpecTaskDefaultsLogDriver, GenericResourcesInnerDiscreteResourceSpec,
  Network, GenericResourcesInner, GenericResourcesInnerNamedResourceSpec,
  NetworkContainer, Ipam, IpamConfig,
};
use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::generic::GenericCount;
use nanocl_stubs::system::{Version, HostInfo};
use nanocl_stubs::metric::{Metric, MetricKind};
use nanocl_stubs::http_metric::HttpMetric;
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
use nanocl_stubs::dns::{ResourceDnsRule, DnsEntry};
use nanocl_stubs::proxy::{
  ResourceProxyRule, ProxyRuleHttp, ProxyHttpLocation, ProxySslConfig,
  ProxyRuleStream, StreamTarget, ProxyStreamProtocol, UriTarget,
  LocationTarget, HttpTarget, UrlRedirect, CargoTarget, ProxyRule, UnixTarget,
};

use super::{
  node, system, namespace, cargo, cargo_image, vm, vm_image, resource, metric,
  http_metric,
};

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

struct EmptyObject;

impl<'__s> utoipa::ToSchema<'__s> for EmptyObject {
  fn schema() -> (
    &'__s str,
    utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
  ) {
    (
      "EmptyObject",
      utoipa::openapi::ObjectBuilder::new()
        .nullable(true)
        .title(Some("EmptyObject"))
        .description(Some("EmptyObject"))
        .schema_type(utoipa::openapi::schema::SchemaType::Object)
        .build()
        .into(),
    )
  }
}

struct GenericResources;

impl<'__s> utoipa::ToSchema<'__s> for GenericResources {
  fn schema() -> (
    &'__s str,
    utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
  ) {
    ("GenericResources", GenericResourcesInner::schema().1)
  }
}

struct BollardDate;

impl<'__s> utoipa::ToSchema<'__s> for BollardDate {
  fn schema() -> (
    &'__s str,
    utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
  ) {
    (
      "BollardDate",
      utoipa::openapi::ObjectBuilder::new()
        .nullable(true)
        .title(Some("BollardDate"))
        .description(Some("BollardDate"))
        .schema_type(utoipa::openapi::schema::SchemaType::String)
        .example(Some("2021-01-01T00:00:00.000000000Z".into()))
        .build()
        .into(),
    )
  }
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
      .default_value("v0.6")
      .description(Some("API version"))
      .enum_values(Some(vec!["v0.6", "v0.5", "v0.4", "v0.3", "v0.2", "v0.1"]))
      .build();

    let server = utoipa::openapi::ServerBuilder::default()
      .url("/{Version}")
      .parameter("Version", variable)
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
    system::get_version,
    system::get_ping,
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
    cargo::restart_cargo,
    cargo::put_cargo,
    cargo::patch_cargo,
    cargo::exec_command,
    cargo::kill_cargo,
    cargo::list_cargo_history,
    cargo::revert_cargo,
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
    resource::revert_resource,
    // Metric
    metric::list_metric,
    // Http Metric
    http_metric::list_http_metric,
    http_metric::count_http_metric,
  ),
  components(schemas(
    // Node
    Node,
    NodeContainerSummary,
    // System
    Version,
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
    GenericResourcesInnerDiscreteResourceSpec,
    GenericResourcesInnerNamedResourceSpec,
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
    NetworkContainer,
    Ipam,
    IpamConfig,
    // Network
    Network,
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
    // ProxyRules
    ResourceProxyRule,
    ProxyRule,
    ProxyRuleHttp,
    ProxyHttpLocation,
    ProxySslConfig,
    ProxyRuleStream,
    StreamTarget,
    ProxyStreamProtocol,
    LocationTarget,
    HttpTarget,
    UrlRedirect,
    CargoTarget,
    UnixTarget,
    UriTarget,
    // DnsRules
    ResourceDnsRule,
    DnsEntry,
    // Metric
    Metric,
    MetricKind,
    // HttpMetric
    HttpMetric,
    // Daemon
    DaemonConfig,
    // Error
    ApiError,
    // Generic Types
    GenericCount,
    Any,
    BollardDate,
    EmptyObject,
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
    (name = "Metrics", description = "Metrics management endpoints."),
    (name = "HttpMetrics", description = "HTTP Metrics management endpoints."),
  ),
  modifiers(&VersionModifier),
)]
pub(crate) struct ApiDoc;
