use std::collections::{BTreeMap, HashMap};

pub use bollard_next::container::Config;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "utoipa")]
use super::generic::Any;

/// Pinned upstream schema for structured cloud-config user-data.
/// NanoCL publishes this reference for clients; the daemon does not fetch it.
pub const CLOUD_INIT_CLOUD_CONFIG_SCHEMA_URL: &str = "https://raw.githubusercontent.com/canonical/cloud-init/26.2/cloudinit/config/schemas/schema-cloud-config-v1.json";

/// Complete cloud-init user-data, supplied as raw text or a cloud-config object.
/// The rendered UTF-8 payload must be at most 64 KiB. It replaces generated user
/// setup; NanoCL retains its instance ID and DHCP network configuration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum VmCloudInitUserData {
  /// Verbatim user-data, including shell scripts and multipart documents.
  Raw(String),
  /// Cloud-config keys as defined by the pinned upstream schema. NanoCL
  /// renders this object with a #cloud-config header; the guest cloud-init
  /// version determines which modules and values it supports.
  CloudConfig(BTreeMap<String, serde_json::Value>),
}

impl From<String> for VmCloudInitUserData {
  fn from(value: String) -> Self {
    Self::Raw(value)
  }
}

#[cfg(any(feature = "utoipa", feature = "schemars"))]
fn cloud_init_user_data_schema() -> serde_json::Value {
  serde_json::json!({
    "description": "Complete cloud-init user-data: raw text or a cloud-config object. Replaces generated user setup; rendered UTF-8 is limited to 64 KiB. The upstream schema documents the object format; NanoCL does not validate guest module semantics.",
    "oneOf": [
      {"type": "string", "description": "Verbatim user-data, including an empty string"},
      {"$ref": CLOUD_INIT_CLOUD_CONFIG_SCHEMA_URL}
    ]
  })
}

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for VmCloudInitUserData {
  fn name() -> std::borrow::Cow<'static, str> {
    "VmCloudInitUserData".into()
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for VmCloudInitUserData {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    serde_json::from_value(cloud_init_user_data_schema())
      .expect("the static cloud-init user-data OpenAPI schema must be valid")
  }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for VmCloudInitUserData {
  fn schema_name() -> String {
    "VmCloudInitUserData".to_owned()
  }

  fn json_schema(
    _generator: &mut schemars::r#gen::SchemaGenerator,
  ) -> schemars::schema::Schema {
    serde_json::from_value(cloud_init_user_data_schema())
      .expect("the static cloud-init user-data JSON schema must be valid")
  }
}

/// A vm's resources (cpu, memory, network)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct VmHostConfig {
  /// Number of cpu of the vm (default: 1)
  pub cpu: u64,
  /// Memory of the vm in MB (default: 512)
  pub memory: u64,
  /// Network interface of the vm to setup (default: ens3)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub net_iface: Option<String>,
  /// Docker uplink interface (default: discovered by the runtime).
  /// If supplied, it must match the network's container interface prefix plus 0.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub link_net_iface: Option<String>,
  /// Enable KVM acceleration (default: false)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub kvm: Option<bool>,
  /// A list of DNS servers for the vm to use
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub dns: Option<Vec<String>>,
  /// Container image name to use for vm (default: nanocl-qemu)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub runtime: Option<String>,
  /// Container network used by the VM runtime.
  #[cfg_attr(
    feature = "serde",
    serde(
      rename = "NetworkMode",
      alias = "RuntimeNetwork",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub runtime_network: Option<String>,
  /// Use host tun device
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub host_tun: Option<bool>,
}

impl Default for VmHostConfig {
  fn default() -> Self {
    Self {
      cpu: 1,
      memory: 512,
      net_iface: None,
      kvm: None,
      dns: None,
      runtime: None,
      host_tun: None,
      link_net_iface: None,
      runtime_network: None,
    }
  }
}

/// A vm spec partial is used to create a vm
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct VmSpecPartial {
  /// Name of the vm
  pub name: String,
  /// The metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Hostname of the vm (default: generated from name)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hostname: Option<String>,
  /// Default user of the vm (default: cloud).
  /// With CloudInit enabled, use lowercase ASCII letters, digits, underscores
  /// or hyphens, starting with a letter or underscore.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub user: Option<String>,
  /// Default password of the vm (default: cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub password: Option<String>,
  /// Default ssh pub key for the user (recommended)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssh_key: Option<String>,
  /// Action to run before the VM runtime container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// Enable cloud-init provisioning (default: true for compatibility).
  /// Guest networking uses DHCP independently of this setting.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cloud_init: Option<bool>,
  /// Complete cloud-init user-data as raw text or a cloud-config object.
  /// Replaces generated user setup; rendered UTF-8 is limited to 64 KiB.
  /// NanoCL retains its instance ID and DHCP network configuration.
  /// Stored but ignored when CloudInit is false.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cloud_init_user_data: Option<VmCloudInitUserData>,
  /// Enable the authenticated browser console (default: false).
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub console: Option<bool>,
  /// The image the vm will use, it must be a full path to the image (ex: /var/lib/nanocl/vm/image.qcow2)
  pub image: String,
  /// Mac address of the vm (default: generated)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub host_config: Option<VmHostConfig>,
}

/// ## VmSpecUpdate
///
/// Payload used to patch a vm
/// It will create a new [VmSpec](VmSpec) with the new values
/// and keep the old values in the history
///
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct VmSpecUpdate {
  /// Name of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  /// The metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Hostname of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hostname: Option<String>,
  /// Default user of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub user: Option<String>,
  /// Default password of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub password: Option<String>,
  /// Default ssh key for the user
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssh_key: Option<String>,
  /// Action to run before the VM runtime container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// Enable cloud-init provisioning; omitted preserves the current setting.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cloud_init: Option<bool>,
  /// Complete cloud-init user-data as raw text or a cloud-config object.
  /// Replaces generated user setup; rendered UTF-8 is limited to 64 KiB.
  /// Omitted or null preserves the current data; an empty string replaces it.
  /// Stored but ignored when CloudInit is false.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cloud_init_user_data: Option<VmCloudInitUserData>,
  /// Enable the authenticated browser console; omitted preserves the setting.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub console: Option<bool>,
  /// Stable MAC identity. An existing address cannot be changed; this may
  /// supply an address for a legacy VM that does not have one yet.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub host_config: Option<VmHostConfig>,
}

impl From<VmSpecPartial> for VmSpecUpdate {
  fn from(spec: VmSpecPartial) -> Self {
    Self {
      name: Some(spec.name),
      hostname: spec.hostname,
      user: spec.user,
      labels: spec.labels,
      host_config: spec.host_config,
      password: spec.password,
      ssh_key: spec.ssh_key,
      init_container: spec.init_container,
      cloud_init: spec.cloud_init,
      cloud_init_user_data: spec.cloud_init_user_data,
      console: spec.console,
      mac_address: spec.mac_address,
      metadata: spec.metadata,
    }
  }
}

/// A vm spec is the specification of a vm
/// It used to know the state of the vm
/// It keep tracking of an history when you patch an existing vm
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmSpec {
  /// Unique identifier of the vm spec
  pub key: uuid::Uuid,
  /// Creation date of the vm spec
  pub created_at: chrono::NaiveDateTime,
  /// Name of the vm
  pub name: String,
  /// Version of the spec
  pub version: String,
  /// Canonical VM key in `{namespace}.{name}` format
  pub vm_key: String,
  /// The metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Hostname of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hostname: Option<String>,
  /// Default password of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub password: Option<String>,
  /// Default ssh key for the user
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssh_key: Option<String>,
  /// Action to run before the VM runtime container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// Enable cloud-init provisioning (default: true for compatibility).
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cloud_init: Option<bool>,
  /// Complete cloud-init user-data as raw text or a cloud-config object.
  /// Replaces generated user setup; rendered UTF-8 is limited to 64 KiB.
  /// NanoCL retains its instance ID and DHCP network configuration.
  /// Stored but ignored when CloudInit is false.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cloud_init_user_data: Option<VmCloudInitUserData>,
  /// Enable the authenticated browser console (default: false).
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub console: Option<bool>,
  /// Default user of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub user: Option<String>,
  /// The image the vm will use, it must be a full path to the image (ex: /var/lib/nanocl/vm/image.qcow2)
  pub image: String,
  /// Mac address of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  pub host_config: VmHostConfig,
}

impl From<VmSpec> for VmSpecUpdate {
  fn from(spec: VmSpec) -> Self {
    Self {
      name: Some(spec.name),
      hostname: spec.hostname,
      user: spec.user,
      labels: spec.labels,
      host_config: Some(spec.host_config),
      password: spec.password,
      ssh_key: spec.ssh_key,
      init_container: spec.init_container,
      cloud_init: spec.cloud_init,
      cloud_init_user_data: spec.cloud_init_user_data,
      console: spec.console,
      mac_address: spec.mac_address,
      metadata: spec.metadata,
    }
  }
}

impl From<VmSpec> for VmSpecPartial {
  fn from(spec: VmSpec) -> Self {
    Self {
      name: spec.name,
      hostname: spec.hostname,
      user: spec.user,
      labels: spec.labels,
      host_config: Some(spec.host_config),
      password: spec.password,
      ssh_key: spec.ssh_key,
      init_container: spec.init_container,
      cloud_init: spec.cloud_init,
      cloud_init_user_data: spec.cloud_init_user_data,
      console: spec.console,
      metadata: spec.metadata,
      image: spec.image,
      mac_address: spec.mac_address,
    }
  }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
  use super::*;

  #[test]
  fn vm_optional_provisioning_and_console_preserve_wire_values() {
    let legacy: VmSpecPartial = serde_json::from_value(serde_json::json!({
      "Name": "legacy", "Image": "/vm.qcow2"
    }))
    .unwrap();
    assert_eq!(legacy.cloud_init, None);
    assert_eq!(legacy.cloud_init_user_data, None);
    assert_eq!(legacy.console, None);
    let spec: VmSpecPartial = serde_json::from_value(serde_json::json!({
      "Name": "plain", "Image": "/vm.qcow2", "CloudInit": false,
      "Console": true, "MacAddress": "02:00:00:00:00:01"
    }))
    .unwrap();
    let update = VmSpecUpdate::from(spec.clone());
    assert_eq!(update.cloud_init, Some(false));
    assert_eq!(update.console, Some(true));
    assert_eq!(update.mac_address.as_deref(), Some("02:00:00:00:00:01"));
    let encoded = serde_json::to_value(spec).unwrap();
    assert_eq!(encoded["CloudInit"], false);
    assert_eq!(encoded["Console"], true);
    let absent = serde_json::to_value(VmSpecUpdate::default()).unwrap();
    assert!(absent.get("CloudInit").is_none());
    assert!(absent.get("CloudInitUserData").is_none());
    assert!(absent.get("Console").is_none());
    let stored = VmSpec {
      cloud_init: Some(false),
      console: Some(true),
      ..Default::default()
    };
    let partial = VmSpecPartial::from(stored.clone());
    let update = VmSpecUpdate::from(stored);
    assert_eq!(partial.cloud_init, update.cloud_init);
    assert_eq!(partial.console, update.console);
  }

  #[test]
  fn vm_cloud_init_user_data_preserves_raw_and_structured_wire_values() {
    for data in [
      serde_json::json!(""),
      serde_json::json!(
        "#cloud-config\nwrite_files:\n- path: /etc/motd\n  content: café\n"
      ),
      serde_json::json!({}),
      serde_json::json!({"packages": ["curl"], "write_files": [{"path": "/etc/motd", "content": "café\n"}]}),
    ] {
      let partial: VmSpecPartial = serde_json::from_value(serde_json::json!({
        "Name": "custom", "Image": "/vm.qcow2", "CloudInit": false,
        "CloudInitUserData": data
      }))
      .unwrap();
      let expected: VmCloudInitUserData =
        serde_json::from_value(data.clone()).unwrap();
      assert_eq!(partial.cloud_init_user_data.as_ref(), Some(&expected));
      let update = VmSpecUpdate::from(partial.clone());
      assert_eq!(update.cloud_init_user_data.as_ref(), Some(&expected));
      assert_eq!(
        serde_json::to_value(&partial).unwrap()["CloudInitUserData"],
        data
      );
      assert_eq!(
        serde_json::to_value(&update).unwrap()["CloudInitUserData"],
        data
      );

      let stored = VmSpec {
        cloud_init: partial.cloud_init,
        cloud_init_user_data: partial.cloud_init_user_data,
        ..Default::default()
      };
      assert_eq!(
        serde_json::to_value(&stored).unwrap()["CloudInitUserData"],
        data
      );
      assert_eq!(
        VmSpecPartial::from(stored.clone())
          .cloud_init_user_data
          .as_ref(),
        Some(&expected)
      );
      assert_eq!(
        VmSpecUpdate::from(stored).cloud_init_user_data.as_ref(),
        Some(&expected)
      );
    }
    let null: VmSpecUpdate = serde_json::from_value(serde_json::json!({
      "CloudInitUserData": null
    }))
    .unwrap();
    assert_eq!(null.cloud_init_user_data, None);
  }

  #[test]
  fn vm_cloud_init_user_data_rejects_scalars_and_arrays() {
    for invalid in [
      serde_json::json!(false),
      serde_json::json!(42),
      serde_json::json!([]),
    ] {
      assert!(
        serde_json::from_value::<VmSpecPartial>(serde_json::json!({
          "Name": "guest", "Image": "/vm.qcow2", "CloudInitUserData": invalid
        }))
        .is_err()
      );
    }
  }

  #[test]
  fn vm_cloud_init_user_data_accepts_structured_statefile_yaml() {
    let state: crate::statefile::Statefile = serde_yaml::from_str(
      "ApiVersion: v0.19.0\nVirtualMachines:\n- Name: guest\n  Image: /vm.qcow2\n  CloudInitUserData:\n    packages: [curl]\n    write_files:\n    - path: /etc/motd\n      content: café\n",
    ).unwrap();
    let data = state
      .virtual_machines
      .unwrap()
      .remove(0)
      .cloud_init_user_data
      .unwrap();
    let VmCloudInitUserData::CloudConfig(config) = data else {
      panic!("expected a cloud-config object");
    };
    assert_eq!(config["packages"], serde_json::json!(["curl"]));
    assert_eq!(config["write_files"][0]["content"], "café");
  }

  #[cfg(feature = "utoipa")]
  #[test]
  fn vm_cloud_init_user_data_openapi_references_upstream_schema() {
    let schema = serde_json::to_value(
      <VmCloudInitUserData as utoipa::PartialSchema>::schema(),
    )
    .unwrap();
    assert_eq!(schema["oneOf"][0]["type"], "string");
    assert_eq!(
      schema["oneOf"][1]["$ref"],
      CLOUD_INIT_CLOUD_CONFIG_SCHEMA_URL
    );
    assert_eq!(schema["oneOf"].as_array().unwrap().len(), 2);
  }

  #[cfg(feature = "schemars")]
  #[test]
  fn vm_cloud_init_user_data_statefile_schema_references_upstream_schema() {
    let schema =
      serde_json::to_value(schemars::schema_for!(crate::statefile::Statefile))
        .unwrap();
    let data = &schema["definitions"]["VmCloudInitUserData"];
    assert_eq!(data["oneOf"][0]["type"], "string");
    assert_eq!(data["oneOf"][1]["$ref"], CLOUD_INIT_CLOUD_CONFIG_SCHEMA_URL);
    assert_eq!(data["oneOf"].as_array().unwrap().len(), 2);
  }
}
