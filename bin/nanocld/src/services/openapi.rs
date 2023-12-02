use ntex::util::HashMap;
use serde::{Serialize, Deserialize};
use utoipa::{OpenApi, Modify, ToSchema};

use bollard_next::exec::StartExecOptions;
use bollard_next::container::{
  Config, ThrottlingData, CPUUsage, BlkioStatsEntry, MemoryStats,
  MemoryStatsStats, PidsStats, NetworkStats, BlkioStats, CPUStats,
  StorageStats, MemoryStatsStatsV1, MemoryStatsStatsV2,
};
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
  NetworkContainer, Ipam, IpamConfig, ExecInspectResponse, ProcessConfig,
  ContainerInspectResponse, ContainerState, NetworkSettings,
  ContainerStateStatusEnum, Health, Address, HealthStatusEnum,
  HealthcheckResult,
};

use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::secret::{Secret, SecretPartial, SecretUpdate};
use nanocl_stubs::generic::{GenericCount, GenericClause, GenericFilter};
use nanocl_stubs::system::{BinaryInfo, HostInfo};
use nanocl_stubs::metric::{Metric, MetricKind};
use nanocl_stubs::http_metric::HttpMetric;
use nanocl_stubs::vm_image::{VmImage, VmImageResizePayload};
use nanocl_stubs::node::{Node, NodeContainerSummary};
use nanocl_stubs::namespace::{
  Namespace, NamespaceSummary, NamespacePartial, NamespaceInspect,
};
use nanocl_stubs::job::{Job, JobPartial, JobInspect, JobSummary};
use nanocl_stubs::cargo::{
  Cargo, CargoInspect, CargoSummary, CargoKillOptions, CreateExecOptions,
  CargoScale, CargoStats,
};
use nanocl_stubs::cargo_spec::{
  CargoSpec, CargoSpecPartial, CargoSpecUpdate, ReplicationMode,
  ReplicationStatic,
};
use nanocl_stubs::cargo_image::CargoImagePartial;
use nanocl_stubs::vm::{Vm, VmInspect, VmSummary};
use nanocl_stubs::vm_spec::{
  VmSpec, VmSpecPartial, VmSpecUpdate, VmDisk, VmHostConfig,
};
use nanocl_stubs::resource::{
  Resource, ResourceUpdate, ResourceSpec, ResourcePartial,
};
use nanocl_stubs::dns::{ResourceDnsRule, DnsEntry};
use nanocl_stubs::proxy::{
  ResourceProxyRule, ProxyRuleHttp, ProxyHttpLocation, ProxySsl,
  ProxyRuleStream, StreamTarget, ProxyStreamProtocol, UriTarget,
  LocationTarget, HttpTarget, UrlRedirect, UpstreamTarget, ProxyRule,
  UnixTarget, ProxySslConfig,
};
use nanocl_stubs::state::{Statefile, StatefileArg};

use super::{
  node, system, namespace, exec, cargo, cargo_image, vm, vm_image, resource,
  metric, secret, job, process,
};

/// When returning a [HttpError](HttpError) the status code is stripped and the error is returned as a json object with the message field set to the error message.
#[allow(dead_code)]
#[derive(ToSchema)]
struct ApiError {
  msg: String,
}

/// Helper to generate have Any type for [OpenApi](OpenApi) usefull for dynamic json objects like [ResourceSpec](ResourceSpec)
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
      .default_value("v0.11")
      .description(Some("API version"))
      .build();

    let server = utoipa::openapi::ServerBuilder::default()
      .url("/{Version}")
      .parameter("Version", variable)
      .build();

    openapi.info.title = "Nanocl Daemon".to_owned();
    openapi.info.version = format!("v{}", env!("CARGO_PKG_VERSION"));
    openapi.info.description =
      Some(include_str!("../../specs/readme.md").to_owned());
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
    // Secret
    secret::list_secret,
    secret::inspect_secret,
    secret::create_secret,
    secret::delete_secret,
    secret::patch_secret,
    // Job
    job::list_job,
    job::delete_job,
    job::inspect_job,
    job::create_job,
    job::wait_job,
    // Cargo
    cargo::list_cargo,
    cargo::list_cargo_instance,
    cargo::inspect_cargo,
    cargo::create_cargo,
    cargo::delete_cargo,
    cargo::stop_cargo,
    cargo::restart_cargo,
    cargo::put_cargo,
    cargo::patch_cargo,
    cargo::kill_cargo,
    cargo::list_cargo_history,
    cargo::revert_cargo,
    cargo::scale_cargo,
    cargo::stats_cargo,
    // Exec
    exec::create_exec_command,
    exec::start_exec_command,
    exec::inspect_exec_command,
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
    resource::put_resource,
    resource::list_resource_history,
    resource::revert_resource,
    // Metric
    metric::list_metric,
    // Process
    process::logs_process,
    process::start_process,
  ),
  components(schemas(
    // Node
    Node,
    NodeContainerSummary,
    // Secret
    Secret,
    SecretPartial,
    SecretUpdate,
    // System
    BinaryInfo,
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
    // Job
    Job,
    JobPartial,
    JobInspect,
    JobSummary,
    // Cargo
    Cargo,
    CreateExecOptions,
    CargoKillOptions,
    CargoInspect,
    CargoSpec,
    ReplicationMode,
    CargoSummary,
    CargoSpecPartial,
    CargoSpecUpdate,
    ReplicationStatic,
    CargoScale,
    CargoStats,
    PidsStats,
    NetworkStats,
    BlkioStats,
    CPUStats,
    StorageStats,
    MemoryStats,
    MemoryStatsStats,
    MemoryStatsStatsV1,
    MemoryStatsStatsV2,
    BlkioStatsEntry,
    CPUUsage,
    ThrottlingData,
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
    ExecInspectResponse,
    StartExecOptions,
    ProcessConfig,
    ContainerInspectResponse,
    ContainerState,
    NetworkSettings,
    ContainerStateStatusEnum,
    Health,
    Address,
    HealthStatusEnum,
    HealthcheckResult,
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
    VmSpec,
    VmSpecPartial,
    VmSpecUpdate,
    VmDisk,
    VmHostConfig,
    // Resource
    Resource,
    ResourceUpdate,
    ResourceSpec,
    ResourcePartial,
    // State
    Statefile,
    StatefileArg,
    // ProxyRules
    ResourceProxyRule,
    ProxyRule,
    ProxyRuleHttp,
    ProxyHttpLocation,
    ProxySsl,
    ProxySslConfig,
    ProxyRuleStream,
    StreamTarget,
    ProxyStreamProtocol,
    LocationTarget,
    HttpTarget,
    UrlRedirect,
    UpstreamTarget,
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
    GenericClause,
    GenericFilter,
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
    (name = "Processes", description = "Processes management endpoints."),
  ),
  modifiers(&VersionModifier),
)]
pub(crate) struct ApiDoc;
