#![cfg(feature = "serde")]

use std::collections::BTreeSet;

use nanocl_stubs::{
  cargo_spec::{
    CargoPlacementSpec, CargoPlacementStrategy, CargoSpec, CargoSpecPatch,
    CargoSpecRevision, CargoSpecValidationError, ContainerSpecValidationError,
    PortMap,
  },
  statefile::Statefile,
};

fn parse_spec(yaml: &str) -> CargoSpec {
  serde_yaml::from_str(yaml).expect("CargoSpec fixture must deserialize")
}

#[test]
fn cargo_placement_strategies_round_trip_without_losing_unit_variants() {
  for strategy in [
    CargoPlacementStrategy::Distinct,
    CargoPlacementStrategy::LeastLoaded,
    CargoPlacementStrategy::Balanced,
    CargoPlacementStrategy::UserDefined("rack-aware".to_owned()),
  ] {
    let json = serde_json::to_value(&strategy).unwrap();
    let decoded: CargoPlacementStrategy = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, strategy);
  }
}

fn parse_validate_and_round_trip(yaml: &str) -> CargoSpec {
  let spec = parse_spec(yaml);
  spec.validate().expect("CargoSpec fixture must validate");
  let json = serde_json::to_string(&spec).expect("CargoSpec must serialize");
  let decoded: CargoSpec =
    serde_json::from_str(&json).expect("CargoSpec JSON must deserialize");
  assert_eq!(decoded, spec);
  spec
}

fn validation_error(yaml: &str) -> CargoSpecValidationError {
  parse_spec(yaml)
    .validate()
    .expect_err("CargoSpec fixture must fail validation")
}

fn assert_unknown_field_rejected(field: &str, yaml: &str) {
  let error = serde_yaml::from_str::<CargoSpec>(yaml).unwrap_err();
  assert!(
    error
      .to_string()
      .contains(&format!("unknown field `{field}`")),
    "unexpected error for {field}: {error}"
  );
}

#[test]
fn single_container_uses_final_shape_and_defaults() {
  let spec = parse_validate_and_round_trip(
    r#"
Name: nginx
Containers:
  - Name: nginx
    Image: nginx:latest
"#,
  );

  assert_eq!(spec.name, "nginx");
  assert_eq!(spec.replicas, 1);
  assert!(spec.network_mode.is_none());
  assert!(spec.port_bindings.is_none());
  assert!(spec.secrets.is_empty());
  assert!(spec.init_containers.is_empty());
  assert_eq!(spec.containers.len(), 1);
}

#[test]
fn multi_container_shape_preserves_order_and_cargo_ownership() {
  let spec = parse_validate_and_round_trip(
    r#"
Name: media-center
Replicas: 2
NetworkMode: media-center
Containers:
  - Name: gluetun
    Image: qmcgaw/gluetun:latest
  - Name: deluge
    Image: lscr.io/linuxserver/deluge:latest
"#,
  );

  assert_eq!(spec.replicas, 2);
  assert_eq!(spec.network_mode.as_ref().unwrap().as_str(), "media-center");
  assert_eq!(
    spec
      .containers
      .iter()
      .map(|container| container.name.as_str())
      .collect::<Vec<_>>(),
    ["gluetun", "deluge"]
  );
}

#[test]
fn cargo_port_bindings_use_the_real_strict_bollard_port_map() {
  let spec = parse_validate_and_round_trip(
    r#"
Name: web
PortBindings:
  "8080/tcp":
    - HostIp: "0.0.0.0"
      HostPort: "8080"
Containers:
  - Name: app
    Image: nginx:latest
"#,
  );

  let ports: PortMap = spec.port_bindings.unwrap();
  let _: bollard_next::models::PortMap = ports.clone();
  let binding = &ports["8080/tcp"].as_ref().unwrap()[0];
  assert_eq!(binding.host_ip.as_deref(), Some("0.0.0.0"));
  assert_eq!(binding.host_port.as_deref(), Some("8080"));
}

#[test]
fn shared_and_container_secrets_remain_separate() {
  let spec = parse_validate_and_round_trip(
    r#"
Name: api
Secrets:
  - common-secret
Containers:
  - Name: api
    Secrets:
      - api-secret
    Image: example/api
"#,
  );

  assert_eq!(spec.secrets, ["common-secret"]);
  assert_eq!(spec.containers[0].secrets, ["api-secret"]);
}

#[test]
fn init_containers_use_container_spec_and_preserve_declaration_order() {
  let spec = parse_validate_and_round_trip(
    r#"
Name: api
InitContainers:
  - Name: migrate
    ImagePullPolicy: IfNotPresent
    Image: example/api
    Cmd:
      - migrate
  - Name: seed
    Image: example/api
    Cmd:
      - seed
Containers:
  - Name: api
    Image: example/api
"#,
  );

  assert_eq!(
    spec
      .init_containers
      .iter()
      .map(|container| container.name.as_str())
      .collect::<Vec<_>>(),
    ["migrate", "seed"]
  );
  assert_eq!(
    spec.init_containers[0].container_config.cmd.as_deref(),
    Some(["migrate".to_owned()].as_slice())
  );
}

#[test]
fn nested_host_and_none_network_escapes_remain_valid() {
  for (name, mode) in [("host-agent", "host"), ("offline", "none")] {
    let yaml = format!(
      "Name: {name}\nContainers:\n  - Name: worker\n    Image: example/worker\n    HostConfig:\n      NetworkMode: {mode}\n"
    );
    let spec = parse_validate_and_round_trip(&yaml);
    assert_eq!(
      spec.containers[0]
        .container_config
        .host_config
        .as_ref()
        .and_then(|host| host.network_mode.as_deref()),
      Some(mode)
    );
  }
}

#[test]
fn missing_and_empty_cargo_names_are_rejected() {
  let missing = serde_yaml::from_str::<CargoSpec>(
    r#"
Containers:
  - Name: app
    Image: nginx
"#,
  )
  .unwrap_err();
  assert!(missing.to_string().contains("Name"));

  assert_eq!(
    validation_error(
      r#"
Name: ""
Containers:
  - Name: app
    Image: nginx
"#,
    ),
    CargoSpecValidationError::EmptyName
  );
}

#[test]
fn replica_count_must_be_positive() {
  let error = validation_error(
    r#"
Name: api
Replicas: 0
Containers:
  - Name: app
    Image: nginx
"#,
  );
  assert_eq!(error, CargoSpecValidationError::InvalidReplicaCount);
  assert!(error.to_string().contains("Replicas"));
}

#[test]
fn containers_are_required_and_must_not_be_empty() {
  let missing = serde_yaml::from_str::<CargoSpec>("Name: api\n").unwrap_err();
  assert!(missing.to_string().contains("Containers"));

  assert_eq!(
    validation_error("Name: api\nContainers: []\n"),
    CargoSpecValidationError::EmptyContainers
  );
}

#[test]
fn container_names_are_unique_across_both_collections() {
  for (yaml, duplicate, first_path, duplicate_path) in [
    (
      r#"
Name: api
Containers:
  - Name: app
    Image: nginx
  - Name: app
    Image: nginx
"#,
      "app",
      "Containers[app]",
      "Containers[app]",
    ),
    (
      r#"
Name: api
InitContainers:
  - Name: migrate
    Image: nginx
  - Name: migrate
    Image: nginx
Containers:
  - Name: app
    Image: nginx
"#,
      "migrate",
      "InitContainers[migrate]",
      "InitContainers[migrate]",
    ),
    (
      r#"
Name: api
InitContainers:
  - Name: app
    Image: nginx
Containers:
  - Name: app
    Image: nginx
"#,
      "app",
      "Containers[app]",
      "InitContainers[app]",
    ),
  ] {
    assert_eq!(
      validation_error(yaml),
      CargoSpecValidationError::DuplicateContainerName {
        name: duplicate.to_owned(),
        first_path: first_path.to_owned(),
        duplicate_path: duplicate_path.to_owned(),
      }
    );
  }
}

#[test]
fn internal_sandbox_name_is_rejected_for_application_containers() {
  let error = validation_error(
    r#"
Name: test
Containers:
  - Name: _sandbox
    Image: nginx
"#,
  );
  assert_eq!(
    error,
    CargoSpecValidationError::ReservedContainerName {
      path: "Containers[_sandbox]".to_owned(),
      name: "_sandbox".to_owned(),
    }
  );
  assert_eq!(
    error.to_string(),
    "CargoSpec.Containers[_sandbox].Name is reserved by Nanocl"
  );
}

#[test]
fn internal_sandbox_name_is_rejected_for_init_containers() {
  let error = validation_error(
    r#"
Name: test
InitContainers:
  - Name: _sandbox
    Image: busybox
Containers:
  - Name: app
    Image: nginx
"#,
  );
  assert_eq!(
    error,
    CargoSpecValidationError::ReservedContainerName {
      path: "InitContainers[_sandbox]".to_owned(),
      name: "_sandbox".to_owned(),
    }
  );
  assert_eq!(
    error.to_string(),
    "CargoSpec.InitContainers[_sandbox].Name is reserved by Nanocl"
  );
}

#[test]
fn names_resembling_the_internal_sandbox_name_remain_valid() {
  for name in ["sandbox", "sandbox-app", "_sandbox-helper"] {
    parse_validate_and_round_trip(&format!(
      "Name: test\nContainers:\n  - Name: {name}\n    Image: nginx\n"
    ));
  }
}

#[test]
fn revision_uses_the_shared_internal_sandbox_name_validation() {
  let spec = parse_spec(
    r#"
Name: test
Containers:
  - Name: _sandbox
    Image: nginx
"#,
  );
  let revision = CargoSpecRevision {
    name: spec.name,
    replicas: spec.replicas,
    init_containers: spec.init_containers,
    containers: spec.containers,
    ..Default::default()
  };

  assert_eq!(
    revision.validate().unwrap_err(),
    CargoSpecValidationError::ReservedContainerName {
      path: "Containers[_sandbox]".to_owned(),
      name: "_sandbox".to_owned(),
    }
  );
}

#[test]
fn applications_require_an_essential_container_and_init_is_always_essential() {
  assert_eq!(
    validation_error(
      r#"
Name: api
Containers:
  - Name: sidecar
    Essential: false
    Image: nginx
"#,
    ),
    CargoSpecValidationError::MissingEssentialContainer
  );

  let error = validation_error(
    r#"
Name: api
InitContainers:
  - Name: migrate
    Essential: false
    Image: nginx
Containers:
  - Name: app
    Image: nginx
"#,
  );
  assert_eq!(
    error,
    CargoSpecValidationError::NonEssentialInitContainer("migrate".to_owned())
  );
  assert!(error.to_string().contains("InitContainers[migrate]"));
}

#[test]
fn nested_container_validation_is_contextualized() {
  let empty_name = validation_error(
    r#"
Name: api
Containers:
  - Name: ""
    Image: nginx
"#,
  );
  assert_eq!(
    empty_name,
    CargoSpecValidationError::InvalidContainer {
      path: "Containers[0]".to_owned(),
      source: ContainerSpecValidationError::EmptyName,
    }
  );

  let invalid_network = validation_error(
    r#"
Name: api
Containers:
  - Name: api
    Image: nginx
    HostConfig:
      NetworkMode: bridge
"#,
  );
  assert_eq!(
    invalid_network,
    CargoSpecValidationError::InvalidContainer {
      path: "Containers[api]".to_owned(),
      source: ContainerSpecValidationError::InvalidNetworkMode,
    }
  );
  assert_eq!(
    invalid_network.to_string(),
    "CargoSpec.Containers[api]: ContainerSpec.HostConfig.NetworkMode must be omitted, \"host\", or \"none\""
  );

  let missing_image = validation_error(
    r#"
Name: api
InitContainers:
  - Name: migrate
Containers:
  - Name: api
    Image: nginx
"#,
  );
  assert_eq!(
    missing_image,
    CargoSpecValidationError::InvalidContainer {
      path: "InitContainers[migrate]".to_owned(),
      source: ContainerSpecValidationError::MissingImage,
    }
  );
  assert_eq!(
    missing_image.to_string(),
    "CargoSpec.InitContainers[migrate]: ContainerSpec.Image is required"
  );
}

#[test]
fn invalid_cargo_network_modes_are_rejected_during_deserialization() {
  for mode in ["  ", "container:other-id"] {
    let yaml = format!(
      "Name: api\nNetworkMode: \"{mode}\"\nContainers:\n  - Name: app\n    Image: nginx\n"
    );
    let error = serde_yaml::from_str::<CargoSpec>(&yaml).unwrap_err();
    assert!(
      error.to_string().contains("CargoSpec.NetworkMode"),
      "unexpected NetworkMode error: {error}"
    );
  }
}

#[test]
fn malformed_or_unknown_cargo_port_bindings_are_rejected_strictly() {
  let malformed = serde_yaml::from_str::<CargoSpec>(
    r#"
Name: api
PortBindings: []
Containers:
  - Name: app
    Image: nginx
"#,
  )
  .unwrap_err();
  assert!(
    malformed.to_string().contains("PortBindings"),
    "unexpected malformed PortBindings error: {malformed}"
  );

  let unknown = serde_yaml::from_str::<CargoSpec>(
    r#"
Name: api
PortBindings:
  "8080/tcp":
    - HostPrt: "8080"
Containers:
  - Name: app
    Image: nginx
"#,
  )
  .unwrap_err();
  assert!(
    unknown
      .to_string()
      .contains("PortBindings.8080/tcp.0.HostPrt"),
    "unexpected strict PortBindings error: {unknown}"
  );
}

#[test]
fn every_removed_or_unknown_cargo_field_is_rejected() {
  for (field, extra) in [
    ("Container", "Container:\n  Image: nginx\n"),
    ("InitContainer", "InitContainer:\n  Image: example/init\n"),
    ("ImagePullPolicy", "ImagePullPolicy: Always\n"),
    ("ImagePullSecret", "ImagePullSecret: registry\n"),
    ("Unknown", "Unknown: value\n"),
  ] {
    let yaml = format!(
      "Name: api\n{extra}Containers:\n  - Name: app\n    Image: nginx\n"
    );
    assert_unknown_field_rejected(field, &yaml);
  }

  assert_unknown_field_rejected(
    "Replicas",
    r#"
Name: api
Placement:
  Replicas: 2
Containers:
  - Name: app
    Image: nginx
"#,
  );
}

#[test]
fn grouped_statefile_uses_the_final_cargo_spec() {
  let state: Statefile = serde_yaml::from_str(
    r#"
ApiVersion: v0.18
Cargoes:
  - Name: api
    Containers:
      - Name: app
        Image: nginx
"#,
  )
  .unwrap();

  let cargoes: Vec<CargoSpec> = state.cargoes.unwrap();
  assert_eq!(cargoes.len(), 1);
  cargoes[0].validate().unwrap();
}

#[test]
fn revision_preserves_metadata_and_uses_the_final_declaration_shape() {
  let json = r#"{
    "Key":"00000000-0000-0000-0000-000000000001",
    "CargoKey":"global.api",
    "Version":"v2",
    "CreatedAt":"2026-08-06T12:34:56",
    "Name":"api",
    "Replicas":2,
    "NetworkMode":"private-api",
    "Secrets":["common-secret"],
    "InitContainers":[],
    "Containers":[{
      "Name":"app",
      "Image":"nginx"
    }]
  }"#;
  let revision: CargoSpecRevision = serde_json::from_str(json).unwrap();
  revision.validate().unwrap();
  assert_eq!(revision.cargo_key, "global.api");
  assert_eq!(revision.replicas, 2);
  assert_eq!(revision.containers[0].name, "app");

  let encoded = serde_json::to_string(&revision).unwrap();
  let decoded: CargoSpecRevision = serde_json::from_str(&encoded).unwrap();
  assert_eq!(decoded, revision);
}

#[test]
fn patch_collections_have_whole_field_replacement_shape() {
  let patch: CargoSpecPatch = serde_yaml::from_str(
    r#"
Replicas: 2
Secrets:
  - replacement-secret
PortBindings:
  "8080/tcp":
    - HostPort: "8080"
InitContainers: []
Containers:
  - Name: replacement
    Image: nginx
"#,
  )
  .unwrap();

  assert_eq!(patch.replicas, Some(2));
  assert_eq!(patch.secrets.unwrap(), ["replacement-secret"]);
  assert_eq!(patch.init_containers, Some(Vec::new()));
  assert_eq!(patch.containers.unwrap()[0].name, "replacement");
  let ports: bollard_next::models::PortMap = patch.port_bindings.unwrap();
  assert!(ports.contains_key("8080/tcp"));

  let legacy = serde_yaml::from_str::<CargoSpecPatch>(
    r#"
Container:
  Image: nginx
"#,
  )
  .unwrap_err();
  assert!(legacy.to_string().contains("unknown field `Container`"));
}

fn required_fields(schema: &serde_json::Value) -> BTreeSet<&str> {
  schema
    .get("required")
    .and_then(serde_json::Value::as_array)
    .into_iter()
    .flatten()
    .map(|field| field.as_str().unwrap())
    .collect()
}

fn property_names(schema: &serde_json::Value) -> BTreeSet<&str> {
  schema["properties"]
    .as_object()
    .expect("object schema must have properties")
    .keys()
    .map(String::as_str)
    .collect()
}

fn assert_final_cargo_schema(schema: &serde_json::Value) {
  assert_eq!(
    required_fields(schema),
    BTreeSet::from(["Containers", "Name"])
  );
  assert_eq!(schema["additionalProperties"], false);
  assert_eq!(
    property_names(schema),
    BTreeSet::from([
      "Containers",
      "InitContainers",
      "Metadata",
      "Name",
      "NetworkMode",
      "Placement",
      "PortBindings",
      "Replicas",
      "ResourceRequirement",
      "Secrets",
    ])
  );
  assert_eq!(schema["properties"]["Replicas"]["default"], 1);
  assert_eq!(
    schema["properties"]["Replicas"]["minimum"].as_f64(),
    Some(1.0)
  );
  assert_eq!(schema["properties"]["Containers"]["type"], "array");
  assert_eq!(schema["properties"]["InitContainers"]["type"], "array");
  for collection in ["Containers", "InitContainers"] {
    let reference = schema["properties"][collection]["items"]["$ref"]
      .as_str()
      .unwrap_or_else(|| {
        panic!("CargoSpec.{collection} must reference ContainerSpec")
      });
    assert_eq!(reference.rsplit('/').next(), Some("ContainerSpec"));
  }
  for removed in [
    "Container",
    "InitContainer",
    "ImagePullPolicy",
    "ImagePullSecret",
    "Key",
    "CargoKey",
    "Version",
    "CreatedAt",
  ] {
    assert!(
      !schema["properties"]
        .as_object()
        .unwrap()
        .contains_key(removed)
    );
  }

  let serialized = serde_json::to_string(schema).unwrap();
  let port_bindings =
    serde_json::to_string(&schema["properties"]["PortBindings"]).unwrap();
  assert!(
    port_bindings.contains("additionalProperties")
      || port_bindings.contains("#/components/schemas/HashMap"),
    "unexpected PortBindings schema: {port_bindings}"
  );
  assert!(!serialized.contains("<port/tcp|udp>"));
}

fn assert_related_cargo_schemas(
  revision: &serde_json::Value,
  patch: &serde_json::Value,
  placement: &serde_json::Value,
) {
  assert_eq!(
    required_fields(revision),
    BTreeSet::from([
      "CargoKey",
      "Containers",
      "CreatedAt",
      "Key",
      "Name",
      "Version",
    ])
  );
  assert_eq!(
    property_names(revision),
    BTreeSet::from([
      "CargoKey",
      "Containers",
      "CreatedAt",
      "InitContainers",
      "Key",
      "Metadata",
      "Name",
      "NetworkMode",
      "Placement",
      "PortBindings",
      "Replicas",
      "ResourceRequirement",
      "Secrets",
      "Version",
    ])
  );
  assert!(required_fields(patch).is_empty());
  assert_eq!(
    property_names(patch),
    BTreeSet::from([
      "Containers",
      "InitContainers",
      "Metadata",
      "Name",
      "NetworkMode",
      "Placement",
      "PortBindings",
      "Replicas",
      "ResourceRequirement",
      "Secrets",
    ])
  );
  assert_eq!(
    property_names(placement),
    BTreeSet::from(["Prefer", "Regions", "Require", "Strategy"])
  );
  assert_eq!(
    patch["properties"]["Replicas"]["minimum"].as_f64(),
    Some(1.0)
  );
  for legacy in [
    "Container",
    "InitContainer",
    "ImagePullPolicy",
    "ImagePullSecret",
  ] {
    assert!(
      !revision["properties"]
        .as_object()
        .unwrap()
        .contains_key(legacy)
    );
    assert!(
      !patch["properties"]
        .as_object()
        .unwrap()
        .contains_key(legacy)
    );
  }
  assert!(
    !placement["properties"]
      .as_object()
      .unwrap()
      .contains_key("Replicas")
  );
}

#[cfg(feature = "schemars")]
#[test]
fn schemars_exposes_only_the_final_cargo_models() {
  let spec = serde_json::to_value(schemars::schema_for!(CargoSpec)).unwrap();
  assert_final_cargo_schema(&spec);
  let spec_json = serde_json::to_string(&spec).unwrap();
  assert!(spec_json.contains("additionalProperties"));
  assert!(spec_json.contains("HostIp"));
  assert!(spec_json.contains("HostPort"));

  let revision =
    serde_json::to_value(schemars::schema_for!(CargoSpecRevision)).unwrap();
  let patch =
    serde_json::to_value(schemars::schema_for!(CargoSpecPatch)).unwrap();
  let placement =
    serde_json::to_value(schemars::schema_for!(CargoPlacementSpec)).unwrap();
  assert_related_cargo_schemas(&revision, &patch, &placement);
}

#[cfg(feature = "utoipa")]
#[test]
fn utoipa_exposes_only_the_final_cargo_models() {
  use utoipa::{PartialSchema, ToSchema};

  let spec = serde_json::to_value(CargoSpec::schema()).unwrap();
  assert_final_cargo_schema(&spec);
  let mut referenced = Vec::new();
  <CargoSpec as ToSchema>::schemas(&mut referenced);
  let referenced = serde_json::to_string(&referenced).unwrap();
  assert!(
    referenced.contains("HostIp"),
    "Utoipa must include Bollard PortBinding components: {referenced}"
  );
  assert!(
    referenced.contains("HostPort"),
    "Utoipa must include Bollard PortBinding components: {referenced}"
  );

  let revision = serde_json::to_value(CargoSpecRevision::schema()).unwrap();
  let patch = serde_json::to_value(CargoSpecPatch::schema()).unwrap();
  let placement = serde_json::to_value(CargoPlacementSpec::schema()).unwrap();
  assert_related_cargo_schemas(&revision, &patch, &placement);
}
