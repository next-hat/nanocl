use utoipa::{Modify, OpenApi, ToSchema};

use nanocl_stubs::{
  dns::ResourceDnsRule, proxy::ResourceProxyRule, statefile::Statefile,
};

use crate::vars;

use super::{
  cargo, event, exec, job, metric, namespace, network, node, process, resource,
  resource_kind, secret, system, vm,
};

/// When returning a [HttpError](nanocl_error::http::HttpError)
/// the status code is stripped and the error
/// is returned as a json object with the message
/// field set to the error message.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct ApiError {
  msg: String,
}

/// Helper to generate the versioned OpenAPI documentation
struct VersionModifier;

impl Modify for VersionModifier {
  fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
    let variable = utoipa::openapi::ServerVariableBuilder::default()
      .default_value(format!("v{}", vars::VERSION))
      .description(Some("API version"))
      .build();
    let server = utoipa::openapi::ServerBuilder::default()
      .url("/{Version}")
      .parameter("Version", variable)
      .build();
    "Nanocl Daemon".clone_into(&mut openapi.info.title);
    openapi.info.version = format!("v{}", vars::VERSION);
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
    node::count_node,
    node::start_node_cargo,
    // Network
    network::list_network,
    network::inspect_network,
    // System
    system::get_info,
    system::get_version,
    system::get_ping,
    // Namespace
    namespace::list_namespace,
    namespace::inspect_namespace,
    namespace::create_namespace,
    namespace::delete_namespace,
    namespace::count_namespace,
    // Secret
    secret::list_secret,
    secret::inspect_secret,
    secret::create_secret,
    secret::delete_secret,
    secret::patch_secret,
    secret::count_secret,
    // Job
    job::list_job,
    job::delete_job,
    job::inspect_job,
    job::create_job,
    job::count_job,
    // Cargo
    cargo::list_cargo,
    cargo::inspect_cargo,
    cargo::create_cargo,
    cargo::delete_cargo,
    cargo::put_cargo,
    cargo::patch_cargo,
    cargo::list_cargo_history,
    cargo::revert_cargo,
    cargo::count_cargo,
    // Exec
    exec::create_exec_command,
    exec::start_exec_command,
    exec::inspect_exec_command,
    // Vm
    vm::list_vm,
    vm::inspect_vm,
    vm::delete_vm,
    vm::create_vm,
    vm::count_vm,
    vm::list_vm_history,
    vm::patch_vm,
    vm::vm_attach,
    // Resource Kind
    resource_kind::list_resource_kind,
    resource_kind::create_resource_kind,
    resource_kind::delete_resource_kind,
    resource_kind::inspect_resource_kind,
    resource_kind::count_resource_kind,
    resource_kind::inspect_resource_kind_version,
    // Resource
    resource::list_resource,
    resource::inspect_resource,
    resource::create_resource,
    resource::delete_resource,
    resource::put_resource,
    resource::list_resource_history,
    resource::revert_resource,
    resource::count_resource,
    // Metric
    metric::list_metric,
    metric::create_metric,
    metric::inspect_metric,
    metric::count_metric,
    // Process
    process::logs_processes,
    process::logs_process,
    process::start_processes,
    process::stop_processes,
    process::list_processes,
    process::restart_processes,
    process::kill_processes,
    process::wait_processes,
    process::stats_processes,
    process::count_processes,
    process::inspect_process,
    process::start_process_by_pk,
    process::process_stats_by_name,
    // Event
    event::list_event,
    event::watch_event,
    event::inspect_event,
    event::count_event,
  ),
  components(schemas(Statefile, ResourceProxyRule, ResourceDnsRule)),
  tags(
    (name = "Namespaces", description = "Namespaces management endpoints."),
    (name = "Networks", description = "Node-scoped Docker network inspection endpoints."),
    (name = "Nodes", description = "Nodes management endpoints."),
    (name = "Resources", description = "Resources management endpoints."),
    (name = "System", description = "General system endpoints."),
    (name = "Vms", description = "Virtual machines management endpoints."),
    (name = "Metrics", description = "Metrics management endpoints."),
    (name = "Processes", description = "Processes management endpoints."),
    (name = "Secrets", description = "Secrets management endpoints."),
    (name = "Jobs", description = "Jobs management endpoints."),
    (name = "Events", description = "Events management endpoints."),
  ),
  modifiers(&VersionModifier),
)]
pub struct ApiDoc;
