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
Container:
  Image: nginx:latest
"#
}

#[test]
fn minimal_container_spec_is_valid_and_keeps_container_boundary() {
  let spec = parse_and_validate(minimal_yaml());
  assert_eq!(spec.name, "app");
  assert!(spec.secrets.is_empty());
  assert_eq!(spec.container.image.as_deref(), Some("nginx:latest"));

  let value = serde_json::to_value(spec).unwrap();
  assert_eq!(value["Name"], "app");
  assert_eq!(value["Container"]["Image"], "nginx:latest");
  assert!(value.get("Image").is_none());
  assert!(value.get("container").is_none());
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
Container:
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
Container:
  Image: nginx:latest
  HostConfig:
    Binds:
      - /srv/config:/config
      - named-volume:/data:ro
"#,
  );
  assert_eq!(
    spec.container.host_config.unwrap().binds.unwrap(),
    ["/srv/config:/config", "named-volume:/data:ro"]
  );
}

#[test]
fn container_network_mode_host_is_valid() {
  let spec = parse_and_validate(
    r#"
Name: host-agent
Container:
  Image: example/host-agent
  HostConfig:
    NetworkMode: host
"#,
  );
  assert_eq!(
    spec
      .container
      .host_config
      .as_ref()
      .unwrap()
      .network_mode
      .as_deref(),
    Some("host")
  );
  let value = serde_json::to_value(spec).unwrap();
  assert_eq!(value["Container"]["HostConfig"]["NetworkMode"], "host");
  assert!(value.get("NetworkMode").is_none());
}

#[test]
fn container_network_mode_none_is_valid() {
  let spec = parse_and_validate(
    r#"
Name: offline-worker
Container:
  Image: example/offline-worker
  HostConfig:
    NetworkMode: none
"#,
  );
  assert_eq!(
    spec.container.host_config.unwrap().network_mode.as_deref(),
    Some("none")
  );
}

#[test]
fn omitted_container_network_mode_is_valid() {
  let spec = parse_and_validate(
    r#"
Name: normal
Container:
  Image: nginx
"#,
  );
  assert!(spec.container.host_config.is_none());
}

fn assert_container_network_mode_rejected(mode: &str) {
  let yaml = format!(
    "Name: app\nContainer:\n  Image: nginx:latest\n  HostConfig:\n    NetworkMode: \"{mode}\"\n"
  );
  let error = validation_error(&yaml);
  assert_eq!(
    error,
    ContainerSpecValidationError::InvalidNetworkMode,
    "container network mode {mode:?} must be rejected"
  );
  assert_eq!(
    error.to_string(),
    "Container.HostConfig.NetworkMode must be omitted, \"host\", or \"none\""
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
Container:
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
Container:
  Image: nginx:latest
"#,
    ),
    ContainerSpecValidationError::EmptyName
  );
}

#[test]
fn missing_container_is_rejected_during_deserialization() {
  let error = serde_yaml::from_str::<ContainerSpec>("Name: app\n").unwrap_err();
  assert!(error.to_string().contains("Container"));
}

#[test]
fn missing_container_image_is_rejected_by_validation() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Container: {}
"#,
    ),
    ContainerSpecValidationError::MissingImage
  );
}

#[test]
fn empty_container_image_is_rejected_by_validation() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Container:
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
Container:
  Image: nginx
  HostConfig:
    PortBindings:
      80/tcp:
        - HostIp: 127.0.0.1
          HostPort: "8080"
"#,
    ),
    ContainerSpecValidationError::ForbiddenField(
      "Container.HostConfig.PortBindings"
    )
  );
}

#[test]
fn publish_all_ports_is_rejected_even_when_false() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Container:
  Image: nginx
  HostConfig:
    PublishAllPorts: false
"#,
    ),
    ContainerSpecValidationError::ForbiddenField(
      "Container.HostConfig.PublishAllPorts"
    )
  );
}

#[test]
fn auto_remove_is_rejected_even_when_false() {
  assert_eq!(
    validation_error(
      r#"
Name: app
Container:
  Image: nginx
  HostConfig:
    AutoRemove: false
"#,
    ),
    ContainerSpecValidationError::ForbiddenField(
      "Container.HostConfig.AutoRemove"
    )
  );
}

#[test]
fn unknown_outer_field_is_rejected() {
  let error = serde_yaml::from_str::<ContainerSpec>(
    r#"
Name: app
Unknown: value
Container:
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
Container:
  Image: nginx
  HostConfig:
    NetworkMdoe: host
"#,
  )
  .unwrap_err();
  assert!(
    error
      .to_string()
      .contains("Container.HostConfig.NetworkMdoe"),
    "unexpected error: {error}"
  );
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
Container:
  Image: nginx
  NetworkDisabled: false
"#,
      "Container.NetworkDisabled",
    ),
    (
      r#"
Name: app
Container:
  Image: nginx
  NetworkingConfig:
    EndpointsConfig: {}
"#,
      "Container.NetworkingConfig",
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
      "Name: app\nContainer:\n  Image: nginx\n  HostConfig:\n    {field}: container:other-id\n"
    );
    assert_eq!(
      validation_error(&yaml),
      ContainerSpecValidationError::ContainerNamespaceReference(match field {
        "IpcMode" => "Container.HostConfig.IpcMode",
        "PidMode" => "Container.HostConfig.PidMode",
        "Cgroup" => "Container.HostConfig.Cgroup",
        "UTSMode" => "Container.HostConfig.UTSMode",
        "UsernsMode" => "Container.HostConfig.UsernsMode",
        _ => unreachable!(),
      })
    );
  }
}

fn required_fields(schema: &serde_json::Value) -> BTreeSet<&str> {
  schema["required"]
    .as_array()
    .expect("object schema must have required fields")
    .iter()
    .map(|field| field.as_str().unwrap())
    .collect()
}

fn assert_container_spec_schema(schema: &serde_json::Value) {
  assert_eq!(
    required_fields(schema),
    BTreeSet::from(["Container", "Name"])
  );
  assert_eq!(schema["additionalProperties"], false);

  let properties = schema["properties"].as_object().unwrap();
  assert_eq!(
    properties
      .keys()
      .map(String::as_str)
      .collect::<BTreeSet<_>>(),
    BTreeSet::from([
      "Container",
      "Essential",
      "ImagePullPolicy",
      "ImagePullSecret",
      "Name",
      "Secrets",
    ])
  );
  assert!(properties.contains_key("Container"));
  assert!(!properties.contains_key("NetworkMode"));
  assert!(!properties.contains_key("Image"));
  assert!(!properties.contains_key("HostConfig"));
}

#[cfg(feature = "schemars")]
#[test]
fn schemars_schema_preserves_required_pascal_case_container_shape() {
  let schema =
    serde_json::to_value(schemars::schema_for!(ContainerSpec)).unwrap();
  assert_container_spec_schema(&schema);

  let cargo_mode =
    serde_json::to_value(schemars::schema_for!(CargoNetworkMode)).unwrap();
  assert_eq!(cargo_mode["type"], "string");
}

#[cfg(feature = "utoipa")]
#[test]
fn utoipa_schema_preserves_required_pascal_case_container_shape() {
  use utoipa::PartialSchema;

  let schema = serde_json::to_value(ContainerSpec::schema()).unwrap();
  assert_container_spec_schema(&schema);

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
Container:
  Image: nginx
  HostConfig:
    Privileged: true
    ReadonlyRootfs: true
"#,
  );
  let host = spec.container.host_config.unwrap();
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
