use utoipa::{Modify, OpenApi};

use nanocl_stubs::{
  dns::ResourceDnsRule, proxy::ResourceProxyRule, statefile::Statefile,
};

pub(crate) use crate::models::ApiError;
use crate::vars;

use super::{
  cargo, event, job, metric, namespace, network, node, process, resource,
  resource_kind, secret, system, vm,
};

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
    // Vm
    vm::list_vm,
    vm::inspect_vm,
    vm::delete_vm,
    vm::create_vm,
    vm::count_vm,
    vm::list_vm_history,
    vm::patch_vm,
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
    process::wait_processes,
    process::stats_processes,
    process::count_processes,
    process::inspect_process,
    process::kill_process,
    process::attach_process,
    process::create_process_exec,
    process::start_process_exec_detached,
    process::start_process_exec_attached,
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

#[cfg(test)]
mod tests {
  use super::*;

  fn generated_openapi_yaml() -> String {
    ApiDoc::openapi()
      .to_yaml()
      .expect("OpenAPI must serialize as YAML")
  }

  fn find_schema_reference(schema: &serde_json::Value) -> Option<&str> {
    match schema {
      serde_json::Value::Array(values) => {
        values.iter().find_map(find_schema_reference)
      }
      serde_json::Value::Object(object) => object
        .get("$ref")
        .and_then(serde_json::Value::as_str)
        .or_else(|| object.values().find_map(find_schema_reference)),
      _ => None,
    }
  }

  #[test]
  fn generated_openapi_uses_closed_flattened_container_spec() {
    let document =
      serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI must serialize");
    let schemas = document["components"]["schemas"]
      .as_object()
      .expect("OpenAPI must contain component schemas");
    let container = &schemas["ContainerSpec"];

    assert_eq!(container["type"], "object");
    assert_eq!(container["additionalProperties"], false);
    assert!(container.get("allOf").is_none());
    assert_eq!(container["required"], serde_json::json!(["Name"]));
    let properties = container["properties"]
      .as_object()
      .expect("ContainerSpec must expose flattened properties");
    for property in [
      "Name",
      "Essential",
      "Secrets",
      "ImagePullSecret",
      "ImagePullPolicy",
      "Image",
      "Cmd",
      "Entrypoint",
      "Env",
      "HostConfig",
      "Healthcheck",
      "Labels",
      "Volumes",
    ] {
      assert!(
        properties.contains_key(property),
        "missing flattened ContainerSpec property {property}"
      );
    }
    for wrapper in ["Container", "container_config"] {
      assert!(!properties.contains_key(wrapper));
    }

    let host_reference = find_schema_reference(&properties["HostConfig"])
      .expect("HostConfig must reference its strict component");
    let host_name = host_reference.rsplit('/').next().unwrap();
    assert_eq!(host_name, "ContainerSpecHostConfig");
    assert_eq!(schemas[host_name]["additionalProperties"], false);
    assert!(
      schemas[host_name]["properties"]
        .get("NetworkMode")
        .is_some()
    );
    let shared_host = schemas
      .get("HostConfig")
      .expect("shared HostConfig must remain registered");
    assert!(shared_host.get("additionalProperties").is_none());

    let cargo = &schemas["CargoSpec"];
    assert_eq!(cargo["required"], serde_json::json!(["Name", "Containers"]));
    let container_reference =
      cargo["properties"]["Containers"]["items"]["$ref"]
        .as_str()
        .expect("CargoSpec.Containers must reference ContainerSpec");
    assert_eq!(container_reference, "#/components/schemas/ContainerSpec");
  }

  #[test]
  fn checked_in_swagger_matches_generated_openapi() {
    let generated = generated_openapi_yaml();
    assert_eq!(
      generated,
      include_str!("../../specs/swagger.yaml"),
      "run the ignored regenerate_checked_in_swagger test"
    );
  }

  #[test]
  fn generated_openapi_exposes_process_attach_and_exec() {
    let document =
      serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI must serialize");
    let paths = document["paths"]
      .as_object()
      .expect("OpenAPI must contain paths");

    assert!(paths.contains_key("/processes/{name}/attach"));
    assert!(!paths.contains_key("/vms/{key}/attach"));
    assert_eq!(
      paths["/processes/{name}/exec"]["post"]["requestBody"]["content"]["application/json"]
        ["schema"]["$ref"],
      "#/components/schemas/ProcessExecCreateOptions"
    );
    assert_eq!(
      paths["/processes/{name}/exec"]["post"]["responses"]["201"]["content"]["application/json"]
        ["schema"]["$ref"],
      "#/components/schemas/ProcessExecCreated"
    );
    assert!(paths["/exec/{id}/start"].get("post").is_some());
    assert_eq!(
      paths["/exec/{id}/start"]["get"]["responses"]["101"]["description"],
      "Attached process exec WebSocket"
    );
  }

  #[test]
  fn generated_openapi_exposes_process_kill() {
    let document =
      serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI must serialize");

    assert_eq!(
      document["paths"]["/processes/{name}/kill"]["post"]["requestBody"]["content"]
        ["application/json"]["schema"]["$ref"],
      "#/components/schemas/ProcessKillOptions"
    );

    assert_eq!(
      document["components"]["schemas"]["ProcessKillOptions"]["required"],
      serde_json::json!(["signal"])
    );
    assert_eq!(
      document["components"]["schemas"]["ProcessKillOptions"]["properties"]["signal"]
        ["type"],
      "string"
    );
  }

  /// Regenerate the checked-in daemon OpenAPI without starting Nanocld.
  #[test]
  #[ignore = "rewrites bin/nanocld/specs/swagger.yaml"]
  fn regenerate_checked_in_swagger() {
    let destination = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("specs/swagger.yaml");
    let yaml = generated_openapi_yaml();
    std::fs::write(destination, yaml)
      .expect("checked-in daemon OpenAPI must be writable");
  }
}
