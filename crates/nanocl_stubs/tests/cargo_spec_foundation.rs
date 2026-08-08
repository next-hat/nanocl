#![cfg(feature = "serde")]

use std::collections::{BTreeSet, HashMap};

use nanocl_stubs::{
  cargo_spec::{
    CargoNetworkMode, ContainerSpec, ContainerSpecValidationError, PortBinding,
    PortMap,
  },
  generic::ImagePullPolicy,
};
use serde::Deserialize;

fn parse_container(yaml: &str) -> ContainerSpec {
  serde_yaml::from_str(yaml).expect("ContainerSpec fixture must deserialize")
}

fn parse_and_validate(yaml: &str) -> ContainerSpec {
  let spec = parse_container(yaml);
  spec
    .validate()
    .expect("ContainerSpec fixture must validate");
  spec
}

fn validation_error(yaml: &str) -> ContainerSpecValidationError {
  parse_container(yaml)
    .validate()
    .expect_err("ContainerSpec fixture must fail validation")
}

fn minimal_yaml() -> &'static str {
  r#"
Name: app
Image: nginx:latest
"#
}

#[test]
fn minimal_container_spec_is_valid_and_serializes_flattened() {
  let spec = parse_and_validate(minimal_yaml());
  assert_eq!(spec.name, "app");
  assert!(spec.secrets.is_empty());
  assert_eq!(spec.container_config.image.as_deref(), Some("nginx:latest"));

  let value = serde_json::to_value(spec).unwrap();
  assert_eq!(value["Name"], "app");
  assert_eq!(value["Image"], "nginx:latest");
  assert!(value.get("Container").is_none());
  assert!(value.get("Config").is_none());
  assert!(value.get("Runtime").is_none());
  assert!(value.get("Docker").is_none());
  assert!(value.get("container").is_none());
  assert!(value.get("container_config").is_none());
}

#[test]
fn essential_defaults_to_true() {
  assert!(parse_container(minimal_yaml()).essential);
}

#[test]
fn image_pull_policy_defaults_to_if_not_present() {
  assert_eq!(
    parse_container(minimal_yaml()).image_pull_policy,
    ImagePullPolicy::IfNotPresent
  );
}

#[test]
fn container_specific_secrets_are_valid() {
  let spec = parse_and_validate(
    r#"
Name: app
Secrets:
  - app-secret
  - database-password
ImagePullSecret: registry-credentials
Image: nginx:latest
"#,
  );
  assert_eq!(spec.secrets, ["app-secret", "database-password"]);
  assert_eq!(
    spec.image_pull_secret.as_deref(),
    Some("registry-credentials")
  );
}

#[test]
fn host_config_binds_remain_valid_and_unrestricted() {
  let spec = parse_and_validate(
    r#"
Name: app
Image: nginx:latest
HostConfig:
  Binds:
    - /srv/config:/config
    - named-volume:/data:ro
"#,
  );
  assert_eq!(
    spec.container_config.host_config.unwrap().binds.unwrap(),
    ["/srv/config:/config", "named-volume:/data:ro"]
  );
}

#[test]
fn container_network_mode_host_is_valid() {
  let spec = parse_and_validate(
    r#"
Name: host-agent
Image: example/host-agent
HostConfig:
  NetworkMode: host
"#,
  );
  assert_eq!(
    spec
      .container_config
      .host_config
      .as_ref()
      .unwrap()
      .network_mode
      .as_deref(),
    Some("host")
  );
  let value = serde_json::to_value(spec).unwrap();
  assert_eq!(value["HostConfig"]["NetworkMode"], "host");
  assert!(value.get("Container").is_none());
}

#[test]
fn container_network_mode_none_is_valid() {
  let spec = parse_and_validate(
    r#"
Name: offline-worker
Image: example/offline-worker
HostConfig:
  NetworkMode: none
"#,
  );
  assert_eq!(
    spec
      .container_config
      .host_config
      .unwrap()
      .network_mode
      .as_deref(),
    Some("none")
  );
}

#[test]
fn omitted_container_network_mode_is_valid() {
  let spec = parse_and_validate(
    r#"
Name: normal
Image: nginx
"#,
  );
  assert!(spec.container_config.host_config.is_none());
}

fn assert_container_network_mode_rejected(mode: &str) {
  let yaml = format!(
    "Name: app\nImage: nginx:latest\nHostConfig:\n  NetworkMode: \"{mode}\"\n"
  );
  let error = validation_error(&yaml);
  assert_eq!(
    error,
    ContainerSpecValidationError::InvalidNetworkMode,
    "container network mode {mode:?} must be rejected"
  );
  assert_eq!(
    error.to_string(),
    "ContainerSpec.HostConfig.NetworkMode must be omitted, \"host\", or \"none\""
  );
}

#[test]
fn arbitrary_container_network_name_is_rejected() {
  for mode in ["nanoclbr0", "default", "private-api", "slirp4netns", "HOST"] {
    assert_container_network_mode_rejected(mode);
  }
}

#[test]
fn bridge_container_network_mode_is_rejected() {
  assert_container_network_mode_rejected("bridge");
}

#[test]
fn container_namespace_network_mode_is_rejected() {
  assert_container_network_mode_rejected("container:abc");
}

#[test]
fn blank_container_network_mode_is_rejected() {
  assert_container_network_mode_rejected("");
  assert_container_network_mode_rejected("  ");
}

#[test]
fn missing_name_is_rejected_during_deserialization() {
  let error = serde_yaml::from_str::<ContainerSpec>(
    r#"
Image: nginx:latest
"#,
  )
  .unwrap_err();
  assert!(error.to_string().contains("Name"));
}

#[test]
fn empty_name_is_rejected_by_validation() {
  assert_eq!(
    validation_error(
      r#"
Name: ""
Image: nginx:latest
"#,
    ),
    ContainerSpecValidationError::EmptyName
  );
}

#[test]
fn missing_image_is_rejected_by_validation() {
  assert_eq!(
    validation_error("Name: app\n"),
    ContainerSpecValidationError::MissingImage
  );
}

#[test]
fn empty_container_image_is_rejected_by_validation() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Image: ""
"#,
    ),
    ContainerSpecValidationError::EmptyImage
  );
}

#[test]
fn container_level_port_bindings_are_rejected_by_presence() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Image: nginx
HostConfig:
  PortBindings:
    80/tcp:
      - HostIp: 127.0.0.1
        HostPort: "8080"
"#,
    ),
    ContainerSpecValidationError::ForbiddenField("HostConfig.PortBindings")
  );
}

#[test]
fn publish_all_ports_is_rejected_even_when_false() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Image: nginx
HostConfig:
  PublishAllPorts: false
"#,
    ),
    ContainerSpecValidationError::ForbiddenField("HostConfig.PublishAllPorts")
  );
}

#[test]
fn auto_remove_is_rejected_even_when_false() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Image: nginx
HostConfig:
  AutoRemove: false
"#,
    ),
    ContainerSpecValidationError::ForbiddenField("HostConfig.AutoRemove")
  );
}

#[test]
fn unknown_outer_field_is_rejected() {
  let error = serde_yaml::from_str::<ContainerSpec>(
    r#"
Name: app
Unknown: value
Image: nginx
"#,
  )
  .unwrap_err();
  assert!(error.to_string().contains("Unknown"));
}

#[test]
fn misspelled_nested_network_mode_is_rejected_with_full_path() {
  let error = serde_yaml::from_str::<ContainerSpec>(
    r#"
Name: app
Image: nginx
HostConfig:
  NetworkMdoe: host
"#,
  )
  .unwrap_err();
  assert!(
    error
      .to_string()
      .contains("ContainerSpec.HostConfig.NetworkMdoe"),
    "unexpected error: {error}"
  );
}

#[test]
fn every_intermediate_container_wrapper_is_rejected() {
  for wrapper in ["Container", "Config", "Runtime", "Docker"] {
    let yaml = format!(
      "Name: app\nImage: nginx\n{wrapper}:\n  Image: nested-is-not-public\n"
    );
    let error = serde_yaml::from_str::<ContainerSpec>(&yaml).unwrap_err();
    assert!(
      error
        .to_string()
        .contains(&format!("ContainerSpec.{wrapper}")),
      "unexpected error for {wrapper}: {error}"
    );
  }
}

#[test]
fn flattened_top_level_typos_are_rejected_with_precise_paths() {
  for (field, yaml) in [
    ("Imgae", "Name: app\nImgae: nginx\n"),
    (
      "ImagePullPolciy",
      "Name: app\nImagePullPolciy: Always\nImage: nginx\n",
    ),
  ] {
    let error = serde_yaml::from_str::<ContainerSpec>(yaml).unwrap_err();
    assert!(
      error
        .to_string()
        .contains(&format!("ContainerSpec.{field}")),
      "unexpected error for {field}: {error}"
    );
  }
}

#[test]
fn duplicate_nanocl_fields_are_rejected_before_bollard_deserialization() {
  for (field, json) in [
    (
      "Name",
      r#"{"Name":"first","Name":"second","Image":"nginx"}"#,
    ),
    (
      "Essential",
      r#"{"Name":"app","Essential":true,"Essential":false,"Image":"nginx"}"#,
    ),
    (
      "Secrets",
      r#"{"Name":"app","Secrets":[],"Secrets":["secret"],"Image":"nginx"}"#,
    ),
    (
      "ImagePullSecret",
      r#"{"Name":"app","ImagePullSecret":null,"ImagePullSecret":"registry","Image":"nginx"}"#,
    ),
    (
      "ImagePullPolicy",
      r#"{"Name":"app","ImagePullPolicy":"Always","ImagePullPolicy":"Never","Image":"nginx"}"#,
    ),
  ] {
    let error = serde_json::from_str::<ContainerSpec>(json).unwrap_err();
    assert!(
      error
        .to_string()
        .contains(&format!("duplicate field `{field}`")),
      "unexpected duplicate error for {field}: {error}"
    );
  }
}

#[test]
fn duplicate_top_level_bollard_fields_are_rejected() {
  let error = serde_json::from_str::<ContainerSpec>(
    r#"{"Name":"app","Image":"first","Image":"second"}"#,
  )
  .unwrap_err();
  assert!(
    error
      .to_string()
      .contains("duplicate Bollard field `Image`")
  );
}

#[test]
fn flattened_container_round_trips_through_yaml_json_and_toml() {
  let spec = parse_and_validate(
    r#"
Name: nginx
Essential: true
Secrets:
  - common-secret
ImagePullPolicy: IfNotPresent
Image: nginx:latest
Cmd:
  - nginx
  - -g
  - daemon off;
HostConfig:
  Binds:
    - /srv/nginx:/etc/nginx:ro
"#,
  );
  let _: &bollard_next::container::Config = &spec.container_config;

  let yaml = serde_yaml::to_string(&spec).unwrap();
  let yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
  assert_eq!(yaml_value["Name"], "nginx");
  assert_eq!(yaml_value["Image"], "nginx:latest");
  assert_eq!(
    yaml_value["HostConfig"]["Binds"][0],
    "/srv/nginx:/etc/nginx:ro"
  );
  assert!(yaml_value.get("Container").is_none());
  assert!(yaml_value.get("container_config").is_none());
  assert_eq!(serde_yaml::from_str::<ContainerSpec>(&yaml).unwrap(), spec);

  let json = serde_json::to_value(&spec).unwrap();
  assert_eq!(json["Name"], "nginx");
  assert_eq!(json["Image"], "nginx:latest");
  assert_eq!(json["HostConfig"]["Binds"][0], "/srv/nginx:/etc/nginx:ro");
  assert!(json.get("Container").is_none());
  assert!(json.get("container_config").is_none());
  assert_eq!(serde_json::from_value::<ContainerSpec>(json).unwrap(), spec);

  let toml = toml::to_string(&spec).unwrap();
  let toml_value: toml::Value = toml::from_str(&toml).unwrap();
  assert_eq!(toml_value["Name"].as_str(), Some("nginx"));
  assert_eq!(toml_value["Image"].as_str(), Some("nginx:latest"));
  assert_eq!(
    toml_value["HostConfig"]["Binds"][0].as_str(),
    Some("/srv/nginx:/etc/nginx:ro")
  );
  assert!(toml_value.get("Container").is_none());
  assert!(toml_value.get("container_config").is_none());
  assert_eq!(toml::from_str::<ContainerSpec>(&toml).unwrap(), spec);
}

#[test]
fn cargo_network_modes_accept_managed_custom_and_builtin_values() {
  for mode in [
    "nanoclbr0",
    "private-api",
    "bridge",
    "default",
    "host",
    "none",
  ] {
    let parsed: CargoNetworkMode = serde_yaml::from_str(mode).unwrap();
    assert_eq!(parsed.as_str(), mode);
    assert_eq!(
      serde_json::to_string(&parsed).unwrap(),
      format!("\"{mode}\"")
    );
  }
}

#[test]
fn cargo_network_mode_rejects_container_namespace_reference() {
  let error =
    serde_json::from_str::<CargoNetworkMode>(r#""container:other-id""#)
      .unwrap_err();
  assert!(error.to_string().contains("container namespace"));
}

#[test]
fn real_bollard_port_map_round_trips_with_pascal_case_bindings() {
  let mut ports = PortMap::new();
  ports.insert(
    "80/tcp".to_owned(),
    Some(vec![PortBinding {
      host_ip: Some("127.0.0.1".to_owned()),
      host_port: Some("8080".to_owned()),
    }]),
  );
  ports.insert("443/tcp".to_owned(), None);

  let json = serde_json::to_string(&ports).unwrap();
  let decoded: bollard_next::models::PortMap =
    serde_json::from_str(&json).unwrap();
  assert_eq!(decoded, ports);

  let value: serde_json::Value = serde_json::from_str(&json).unwrap();
  assert_eq!(value["80/tcp"][0]["HostIp"], "127.0.0.1");
  assert_eq!(value["80/tcp"][0]["HostPort"], "8080");
  assert!(value["80/tcp"][0].get("host_port").is_none());
  assert!(value["443/tcp"].is_null());

  let reexported: PortMap = decoded;
  let _: bollard_next::models::PortMap = reexported;
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "PascalCase")]
struct StrictPortMapFixture {
  #[serde(
    deserialize_with = "nanocl_stubs::cargo_spec::strict_bollard::deserialize_port_map"
  )]
  port_bindings: PortMap,
}

#[test]
fn unknown_port_binding_field_is_rejected_with_nested_path() {
  let error = serde_json::from_str::<StrictPortMapFixture>(
    r#"{
      "PortBindings": {
        "80/tcp": [{ "HostPrt": "8080" }]
      }
    }"#,
  )
  .unwrap_err();
  assert!(
    error.to_string().contains("PortBindings.80/tcp.0.HostPrt"),
    "unexpected error: {error}"
  );
}

#[test]
fn networking_config_and_network_disabled_are_compiler_owned() {
  for (yaml, expected) in [
    (
      r#"
Name: app
Image: nginx
NetworkDisabled: false
"#,
      "NetworkDisabled",
    ),
    (
      r#"
Name: app
Image: nginx
NetworkingConfig:
  EndpointsConfig: {}
"#,
      "NetworkingConfig",
    ),
  ] {
    assert_eq!(
      validation_error(yaml),
      ContainerSpecValidationError::ForbiddenField(expected)
    );
  }
}

#[test]
fn every_raw_container_namespace_reference_is_rejected() {
  for field in ["IpcMode", "PidMode", "Cgroup", "UTSMode", "UsernsMode"] {
    let yaml = format!(
      "Name: app\nImage: nginx\nHostConfig:\n  {field}: container:other-id\n"
    );
    assert_eq!(
      validation_error(&yaml),
      ContainerSpecValidationError::ContainerNamespaceReference(match field {
        "IpcMode" => "HostConfig.IpcMode",
        "PidMode" => "HostConfig.PidMode",
        "Cgroup" => "HostConfig.Cgroup",
        "UTSMode" => "HostConfig.UTSMode",
        "UsernsMode" => "HostConfig.UsernsMode",
        _ => unreachable!(),
      })
    );
  }
}

fn collect_schema_fields(
  schema: &serde_json::Value,
  references: &HashMap<String, serde_json::Value>,
  properties: &mut BTreeSet<String>,
  required: &mut BTreeSet<String>,
  visited: &mut BTreeSet<String>,
) {
  if let Some(fields) =
    schema.get("properties").and_then(|value| value.as_object())
  {
    properties.extend(fields.keys().cloned());
  }
  if let Some(fields) =
    schema.get("required").and_then(|value| value.as_array())
  {
    required.extend(
      fields
        .iter()
        .filter_map(|field| field.as_str().map(str::to_owned)),
    );
  }
  for composition in ["allOf", "anyOf", "oneOf"] {
    if let Some(parts) =
      schema.get(composition).and_then(|value| value.as_array())
    {
      for part in parts {
        collect_schema_fields(part, references, properties, required, visited);
      }
    }
  }
  if let Some(reference) = schema.get("$ref").and_then(|value| value.as_str()) {
    let name = reference.rsplit('/').next().unwrap();
    if visited.insert(name.to_owned()) {
      let referenced = references
        .get(name)
        .unwrap_or_else(|| panic!("missing schema reference {reference}"));
      collect_schema_fields(
        referenced, references, properties, required, visited,
      );
    }
  }
}

fn assert_container_spec_schema(
  schema: &serde_json::Value,
  references: &HashMap<String, serde_json::Value>,
) {
  if let Some(parts) = schema.get("allOf").and_then(|value| value.as_array()) {
    for part in parts {
      assert_ne!(
        part.get("additionalProperties"),
        Some(&serde_json::Value::Bool(false)),
        "an allOf branch cannot reject fields supplied by the other flattened branch"
      );
    }
  }

  let mut properties = BTreeSet::new();
  let mut required = BTreeSet::new();
  collect_schema_fields(
    schema,
    references,
    &mut properties,
    &mut required,
    &mut BTreeSet::new(),
  );

  assert_eq!(required, BTreeSet::from(["Name".to_owned()]));
  for field in [
    "Name",
    "Essential",
    "Secrets",
    "ImagePullSecret",
    "ImagePullPolicy",
    "Image",
    "Cmd",
    "HostConfig",
  ] {
    assert!(
      properties.contains(field),
      "missing flattened field {field}"
    );
  }
  for forbidden in [
    "Container",
    "container_config",
    "Config",
    "Runtime",
    "Docker",
  ] {
    assert!(
      !properties.contains(forbidden),
      "schema exposed forbidden wrapper {forbidden}"
    );
  }
}

#[cfg(feature = "schemars")]
#[test]
fn schemars_schema_preserves_required_pascal_case_container_shape() {
  let schema =
    serde_json::to_value(schemars::schema_for!(ContainerSpec)).unwrap();
  let references = schema["definitions"]
    .as_object()
    .unwrap()
    .iter()
    .map(|(name, schema)| (name.clone(), schema.clone()))
    .collect();
  assert_container_spec_schema(&schema, &references);

  let cargo_mode =
    serde_json::to_value(schemars::schema_for!(CargoNetworkMode)).unwrap();
  assert_eq!(cargo_mode["type"], "string");
}

#[cfg(feature = "utoipa")]
#[test]
fn utoipa_schema_preserves_required_pascal_case_container_shape() {
  use utoipa::{PartialSchema, ToSchema};

  let schema = serde_json::to_value(ContainerSpec::schema()).unwrap();
  let mut referenced = Vec::new();
  <ContainerSpec as ToSchema>::schemas(&mut referenced);
  let references = referenced
    .into_iter()
    .map(|(name, schema)| (name, serde_json::to_value(schema).unwrap()))
    .collect();
  assert_container_spec_schema(&schema, &references);

  let cargo_mode = serde_json::to_value(CargoNetworkMode::schema()).unwrap();
  assert_eq!(cargo_mode["type"], "string");
}

#[test]
fn optional_strict_port_map_adapter_accepts_real_port_maps() {
  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct OptionalPortMapFixture {
    #[serde(
      default,
      deserialize_with = "nanocl_stubs::cargo_spec::strict_bollard::deserialize_optional_port_map"
    )]
    port_bindings: Option<PortMap>,
  }

  let omitted: OptionalPortMapFixture = serde_json::from_str("{}").unwrap();
  assert!(omitted.port_bindings.is_none());
  let present: OptionalPortMapFixture = serde_json::from_str(
    r#"{"PortBindings":{"80/tcp":[{"HostPort":"8080"}]}}"#,
  )
  .unwrap();
  assert_eq!(
    present.port_bindings.unwrap()["80/tcp"].as_ref().unwrap()[0]
      .host_port
      .as_deref(),
    Some("8080")
  );
}

#[test]
fn ordinary_non_network_docker_options_remain_valid() {
  let spec = parse_and_validate(
    r#"
Name: app
Image: nginx
HostConfig:
  Privileged: true
  ReadonlyRootfs: true
"#,
  );
  let host = spec.container_config.host_config.unwrap();
  assert_eq!(host.privileged, Some(true));
  assert_eq!(host.readonly_rootfs, Some(true));
}

#[test]
fn cargo_network_mode_rejects_blank_values() {
  assert!(CargoNetworkMode::new("").is_err());
  assert!(CargoNetworkMode::new("  ").is_err());
}

#[test]
fn strict_port_map_fixture_uses_the_real_port_map_type() {
  let fixture: StrictPortMapFixture = serde_json::from_str(
    r#"{"PortBindings":{"53/udp":[{"HostPort":"5353"}]}}"#,
  )
  .unwrap();
  let binding = &fixture.port_bindings["53/udp"].as_ref().unwrap()[0];
  assert_eq!(binding.host_port.as_deref(), Some("5353"));

  let _: HashMap<String, Option<Vec<bollard_next::models::PortBinding>>> =
    fixture.port_bindings;
}
