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
    openapi
      .components
      .get_or_insert_with(Default::default)
      .schemas
      .insert(
        "SecretEnv".to_owned(),
        utoipa::openapi::ArrayBuilder::new()
          .items(<String as utoipa::PartialSchema>::schema())
          .description(Some(
            "Data for a nanocl.io/env secret: KEY=VALUE entries.",
          ))
          .into(),
      );
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
    node::delete_node_cargo,
    node::start_node_cargo_operation,
    node::delete_node_cargo_operation,
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
    job::list_job_history,
    job::inspect_job_history,
    job::list_job_runs,
    job::inspect_job_run,
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
    vm::console_vm,
    vm::stop_vm,
    vm::restart_vm,
    vm::vm_status,
    vm::resize_vm,
    vm::vm_resources,
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
    process::process_stats_by_name,
    // Event
    event::list_event,
    event::watch_event,
    event::inspect_event,
    event::count_event,
  ),
  components(schemas(nanocl_stubs::node::DeleteNodeCargoOperationParams, Statefile, ResourceProxyRule, ResourceDnsRule, nanocl_stubs::job::JobHistoryQuery, nanocl_stubs::job::JobDeleteQuery)),
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

  #[test]
  fn generated_openapi_separates_job_configurations_and_runs() {
    let document = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let paths = &document["paths"];
    let list = &paths["/jobs/{name}/runs"]["get"];
    assert_eq!(
      list["responses"]["200"]["content"]["application/json"]["schema"]["items"]
        ["$ref"],
      "#/components/schemas/JobRun"
    );
    for query in ["limit", "offset"] {
      assert!(
        list["parameters"]
          .as_array()
          .unwrap()
          .iter()
          .any(|parameter| {
            parameter["name"] == query && parameter["in"] == "query"
          })
      );
    }
    assert_eq!(
      paths["/jobs/{name}/runs/{run}"]["get"]["responses"]["200"]["content"]["application/json"]
        ["schema"]["$ref"],
      "#/components/schemas/JobRunInspect"
    );
    let history = &paths["/jobs/{name}/histories"]["get"];
    assert_eq!(
      history["responses"]["200"]["content"]["application/json"]["schema"]["items"]
        ["$ref"],
      "#/components/schemas/JobSpec"
    );
    assert_eq!(
      paths["/jobs/{name}/histories/{id}"]["get"]["responses"]["200"]["content"]
        ["application/json"]["schema"]["$ref"],
      "#/components/schemas/JobSpec"
    );
    for query in ["limit", "offset"] {
      assert!(history["parameters"].as_array().unwrap().iter().any(
        |parameter| parameter["name"] == query && parameter["in"] == "query"
      ));
    }
    assert!(paths.get("/jobs/{name}/histories/{run}").is_none());
    let checked_in: serde_json::Value =
      serde_yaml::from_str(include_str!("../../specs/swagger.yaml")).unwrap();
    for path in [
      "/jobs/{name}/histories",
      "/jobs/{name}/histories/{id}",
      "/jobs/{name}/runs",
      "/jobs/{name}/runs/{run}",
    ] {
      assert_eq!(checked_in["paths"][path], paths[path], "{path}");
    }
    let schemas = &document["components"]["schemas"];
    for field in ["Key", "JobKey", "Version", "CreatedAt"] {
      assert!(schemas["JobSpec"]["properties"][field].is_object());
      assert!(
        schemas["JobSpec"]["required"]
          .as_array()
          .unwrap()
          .iter()
          .any(|required| required == field)
      );
      assert!(schemas["JobPartial"]["properties"].get(field).is_none());
    }
    for model in ["Job", "JobInspect", "JobSummary"] {
      for field in ["IncarnationKey", "SpecKey", "CurrentRunKey"] {
        assert!(schemas[model]["properties"][field].is_object());
      }
    }
    for model in [
      "JobRun",
      "JobRunStep",
      "JobRunOperation",
      "JobRunInspect",
      "JobHistoryQuery",
    ] {
      assert!(schemas[model].is_object(), "missing {model}");
      assert_eq!(
        checked_in["components"]["schemas"][model], schemas[model],
        "{model}"
      );
      assert_local_schema_references(&document, &schemas[model]);
    }
    assert!(
      schemas["JobRunStep"]["properties"]["ExitCode"]["type"]
        .as_array()
        .unwrap()
        .iter()
        .any(|kind| kind == "null")
    );
    for private in ["OwnerBootId", "NodeName", "LockDevice", "LockInode"] {
      assert!(
        schemas["JobRunOperation"]["properties"]
          .get(private)
          .is_none()
      );
    }
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
  fn generated_openapi_exposes_vm_resize_and_limits() {
    let generated = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let checked_in: serde_json::Value =
      serde_yaml::from_str(include_str!("../../specs/swagger.yaml")).unwrap();
    for path in ["/vms/{key}/resize", "/vms/{key}/resources"] {
      assert_eq!(
        checked_in["paths"][path], generated["paths"][path],
        "{path}"
      );
    }
    for name in [
      "VmResizeOptions",
      "VmResources",
      "VmHostConfig",
      "VmHostConfigUpdate",
    ] {
      assert_eq!(
        checked_in["components"]["schemas"][name],
        generated["components"]["schemas"][name],
        "{name}"
      );
    }
    assert_eq!(
      generated["paths"]["/vms/{key}/resize"]["post"]["requestBody"]["required"],
      true
    );
    assert_eq!(
      generated["components"]["schemas"]["VmResources"]["properties"]["Persistent"]
        ["type"],
      "boolean"
    );
    for field in ["MaxCpu", "MaxMemory"] {
      assert!(
        generated["components"]["schemas"]["VmHostConfig"]["properties"][field]
          .is_object()
      );
      assert!(
        !generated["components"]["schemas"]["VmHostConfig"]["required"]
          .as_array()
          .unwrap()
          .iter()
          .any(|item| item == field)
      );
    }
    assert_eq!(
      find_schema_reference(
        &generated["components"]["schemas"]["VmSpecUpdate"]["properties"]["HostConfig"]
      ),
      Some("#/components/schemas/VmHostConfigUpdate")
    );
    assert!(
      generated["components"]["schemas"]["VmHostConfigUpdate"]["required"]
        .as_array()
        .is_none_or(Vec::is_empty)
    );
  }

  #[test]
  fn generated_openapi_exposes_vm_lifecycle() {
    let generated = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let checked_in: serde_json::Value =
      serde_yaml::from_str(include_str!("../../specs/swagger.yaml")).unwrap();
    for path in ["/vms/{key}/stop", "/vms/{key}/restart", "/vms/{key}/status"] {
      assert_eq!(
        checked_in["paths"][path], generated["paths"][path],
        "{path}"
      );
    }
    for name in ["VmLifecycleOptions", "VmRuntimeStatus"] {
      assert_eq!(
        checked_in["components"]["schemas"][name],
        generated["components"]["schemas"][name],
        "{name}"
      );
    }
    for path in ["/vms/{key}/stop", "/vms/{key}/restart"] {
      let operation = &generated["paths"][path]["post"];
      assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/VmLifecycleOptions"
      );
      assert_eq!(operation["requestBody"]["required"], true);
      assert!(operation["responses"]["504"].is_object());
      assert!(operation["responses"].get("403").is_none());
    }
  }

  #[test]
  fn generated_openapi_exposes_vm_cloud_init_user_data() {
    let generated = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let checked_in: serde_json::Value =
      serde_yaml::from_str(include_str!("../../specs/swagger.yaml")).unwrap();
    let schemas = &generated["components"]["schemas"];
    let user_data = &schemas["VmCloudInitUserData"];
    assert_eq!(user_data["oneOf"][0]["type"], "string");
    assert_eq!(
      user_data["oneOf"][1]["$ref"],
      "#/components/schemas/CloudInit26_2"
    );
    assert_eq!(
      checked_in["components"]["schemas"]["VmCloudInitUserData"],
      *user_data
    );
    for name in ["VmSpecPartial", "VmSpecUpdate", "VmSpec"] {
      assert_eq!(checked_in["components"]["schemas"][name], schemas[name]);
    }
    let cloud_init: Vec<_> = schemas
      .as_object()
      .unwrap()
      .iter()
      .filter(|(name, _)| name.starts_with("CloudInit26_2"))
      .collect();
    assert_eq!(cloud_init.len(), 77);
    for (name, schema) in cloud_init {
      assert_eq!(checked_in["components"]["schemas"][name], *schema, "{name}");
      assert_local_schema_references(&generated, schema);
    }
    for document in [&generated, &checked_in] {
      for name in ["VmSpecPartial", "VmSpecUpdate", "VmSpec"] {
        let schema = &document["components"]["schemas"][name];
        let property = &schema["properties"]["CloudInitUserData"];
        assert_eq!(
          find_schema_reference(property),
          Some("#/components/schemas/VmCloudInitUserData")
        );
        assert!(!schema["required"].as_array().is_some_and(|items| {
          items.iter().any(|item| item == "CloudInitUserData")
        }));
      }
    }
  }

  fn assert_local_schema_references(
    document: &serde_json::Value,
    value: &serde_json::Value,
  ) {
    match value {
      serde_json::Value::Array(items) => {
        for item in items {
          assert_local_schema_references(document, item);
        }
      }
      serde_json::Value::Object(object) => {
        if let Some(reference) = object.get("$ref") {
          let reference = reference.as_str().unwrap();
          // Both the docs and VS Code Statefile converter require local
          // component references, including references nested in cloud-config.
          assert!(reference.starts_with("#/components/schemas/"));
          assert!(document.pointer(&reference[1..]).is_some(), "{reference}");
        }
        for child in object.values() {
          assert_local_schema_references(document, child);
        }
      }
      _ => {}
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
    // Use the current native Docker create model, without legacy top-level MACs.
    assert!(!properties.contains_key("MacAddress"));

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
  fn generated_openapi_exposes_node_delete_query() {
    let generated = serde_json::to_value(ApiDoc::openapi()).unwrap();
    assert_node_delete_query(&generated);
  }

  fn assert_node_delete_query(document: &serde_json::Value) {
    let operation = &document["paths"]["/nodes/cargoes"]["delete"];
    assert!(operation.is_object());
    assert!(operation.get("requestBody").is_none());
    let [parameter] = operation["parameters"].as_array().unwrap().as_slice()
    else {
      panic!("node deletion must require exactly one query parameter");
    };
    assert_eq!(parameter["name"], "CargoKey");
    assert_eq!(parameter["in"], "query");
    assert_eq!(parameter["required"], true);
    assert_eq!(parameter["schema"]["type"], "string");
  }

  #[test]
  fn checked_in_swagger_matches_generated_openapi() {
    // Schema extension maps do not guarantee YAML key order. Compare every
    // value in the parsed document while preserving array order and types.
    let generated = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let checked_in: serde_json::Value =
      serde_yaml::from_str(include_str!("../../specs/swagger.yaml")).unwrap();
    for document in [&generated, &checked_in] {
      let paths = document["paths"].as_object().unwrap();
      assert!(!paths.contains_key("/processes/{pk}/start"));
      assert!(paths["/processes/{kind}/{key}/start"]["post"].is_object());
      assert_node_delete_query(document);
    }
    assert_eq!(
      generated, checked_in,
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

  #[test]
  fn generated_openapi_exposes_optional_process_ip_address() {
    let document =
      serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI must serialize");
    let process = &document["components"]["schemas"]["Process"];
    let properties = &process["properties"];

    assert_eq!(
      properties["IpAddress"]["type"],
      serde_json::json!(["string", "null"])
    );
    assert!(
      !process["required"]
        .as_array()
        .expect("Process must declare required properties")
        .iter()
        .any(|property| property == "IpAddress")
    );
    assert_eq!(
      properties["Data"],
      serde_json::json!({
        "$ref": "#/components/schemas/ContainerInspectResponse",
        "description": "The data of the process a ContainerInspect",
      })
    );

    let checked_in: serde_json::Value =
      serde_yaml::from_str(include_str!("../../specs/swagger.yaml"))
        .expect("checked-in OpenAPI must parse as YAML");
    assert_eq!(
      process, &checked_in["components"]["schemas"]["Process"],
      "checked-in Process schema must match the generated API contract"
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
