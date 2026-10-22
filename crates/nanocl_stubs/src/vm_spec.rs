use std::collections::HashMap;

pub use bollard_next::container::Config;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "utoipa")]
use super::generic::Any;

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
}
