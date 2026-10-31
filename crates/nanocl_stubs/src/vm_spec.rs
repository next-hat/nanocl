use std::collections::{BTreeMap, HashMap};

pub use bollard_next::models::ContainerCreateBody as Config;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "utoipa")]
use super::generic::Any;

/// Pinned upstream schema for structured cloud-config user-data.
/// NanoCL bundles this schema; clients and the daemon do not fetch it.
pub const CLOUD_INIT_CLOUD_CONFIG_SCHEMA_URL: &str = "https://raw.githubusercontent.com/canonical/cloud-init/26.2/cloudinit/config/schemas/schema-cloud-config-v1.json";

#[cfg(any(feature = "utoipa", feature = "schemars"))]
const CLOUD_INIT_SCHEMA_NAME: &str = "CloudInit26_2";

/// Complete cloud-init user-data, supplied as raw text or a cloud-config object.
/// The rendered UTF-8 payload must be at most 64 KiB. It replaces generated user
/// setup; NanoCL retains its instance ID and DHCP network configuration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum VmCloudInitUserData {
  /// Explicit null restores generated user setup. Omitted PATCH data is retained.
  Reset,
  /// Verbatim user-data, including shell scripts and multipart documents.
  Raw(String),
  /// Cloud-config keys as defined by the pinned upstream schema. NanoCL
  /// renders this object with a #cloud-config header; the guest cloud-init
  /// version determines which modules and values it supports.
  CloudConfig(BTreeMap<String, serde_json::Value>),
}

#[cfg(feature = "serde")]
fn deserialize_optional_update<'de, D, T>(
  deserializer: D,
) -> Result<Option<T>, D::Error>
where
  D: serde::Deserializer<'de>,
  T: Deserialize<'de>,
{
  T::deserialize(deserializer).map(Some)
}

impl From<String> for VmCloudInitUserData {
  fn from(value: String) -> Self {
    Self::Raw(value)
  }
}

#[cfg(any(feature = "utoipa", feature = "schemars"))]
fn cloud_init_user_data_schema(reference_prefix: &str) -> serde_json::Value {
  serde_json::json!({
    "description": "Complete cloud-init user-data: raw text or a cloud-config object. Replaces generated user setup; rendered UTF-8 is limited to 64 KiB. The upstream schema documents the object format; NanoCL does not validate guest module semantics.",
    "oneOf": [
      {"type": "string", "maxLength": 65536, "description": "Verbatim user-data, including an empty string. The daemon enforces a 65536-byte UTF-8 limit; maxLength bounds characters, so non-ASCII text may reach the byte limit sooner."},
      {"$ref": format!("{reference_prefix}{CLOUD_INIT_SCHEMA_NAME}")},
      {"type": "null", "description": "Restore generated User/Password/SshKey provisioning"}
    ]
  })
}

#[cfg(any(feature = "utoipa", feature = "schemars"))]
fn cloud_init_schema_components(
  reference_prefix: &str,
) -> BTreeMap<String, serde_json::Value> {
  fn localize(value: &mut serde_json::Value, reference_prefix: &str) {
    match value {
      serde_json::Value::Array(items) => {
        for item in items {
          localize(item, reference_prefix);
        }
      }
      serde_json::Value::Object(object) => {
        if let Some(reference) = object.get_mut("$ref") {
          let name = reference
            .as_str()
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
            .expect("the pinned cloud-init schema uses only local definitions");
          *reference =
            format!("{reference_prefix}{CLOUD_INIT_SCHEMA_NAME}.{name}").into();
        }
        for child in object.values_mut() {
          localize(child, reference_prefix);
        }
      }
      _ => {}
    }
  }

  let mut root: serde_json::Value =
    serde_json::from_str(include_str!("../schemas/cloud-init-26.2.json"))
      .expect("the bundled upstream cloud-init schema must be valid JSON");
  // The pinned upstream module tuple wraps a schema reference in `enum`,
  // whose value must be an array. Refer directly to the existing enum schema.
  // Keep the bundled upstream file unchanged for provenance and licensing.
  let module = root
    .pointer_mut("/$defs/modules_definition/items/oneOf/1/prefixItems/0")
    .expect("the pinned cloud-init module tuple exists");
  assert_eq!(
    module,
    &serde_json::json!({"enum": {"$ref": "#/$defs/all_modules"}})
  );
  *module = serde_json::json!({"$ref": "#/$defs/all_modules"});
  let object = root
    .as_object_mut()
    .expect("cloud-init schema is an object");
  let definitions = object.remove("$defs").expect("cloud-init definitions");
  // The containing document supplies the dialect. Structured user-data is an
  // object; upstream validates only its fields and otherwise accepts scalars.
  object.remove("$schema");
  object.insert("type".into(), "object".into());
  let mut schemas = BTreeMap::new();
  for (name, value) in definitions
    .as_object()
    .expect("cloud-init definitions are an object")
  {
    schemas.insert(format!("{CLOUD_INIT_SCHEMA_NAME}.{name}"), value.clone());
  }
  schemas.insert(CLOUD_INIT_SCHEMA_NAME.to_owned(), root);
  for schema in schemas.values_mut() {
    localize(schema, reference_prefix);
  }
  schemas
}

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for VmCloudInitUserData {
  fn name() -> std::borrow::Cow<'static, str> {
    "VmCloudInitUserData".into()
  }

  fn schemas(
    schemas: &mut Vec<(
      String,
      utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
    )>,
  ) {
    for (name, value) in cloud_init_schema_components("#/components/schemas/") {
      // Utoipa's typed enum cannot retain patternProperties, not, or combined
      // properties/oneOf schemas. Its flattened map preserves every upstream
      // JSON Schema keyword, including nested schemas, during serialization.
      let mut schema = utoipa::openapi::schema::Object::with_type(
        utoipa::openapi::schema::SchemaType::AnyValue,
      );
      schema.extensions = Some(
        value
          .as_object()
          .expect("cloud-init components are objects")
          .clone()
          .into_iter()
          .collect(),
      );
      schemas.push((name, schema.into()));
    }
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for VmCloudInitUserData {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    serde_json::from_value(cloud_init_user_data_schema("#/components/schemas/"))
      .expect("the static cloud-init user-data OpenAPI schema must be valid")
  }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for VmCloudInitUserData {
  fn schema_name() -> std::borrow::Cow<'static, str> {
    "VmCloudInitUserData".into()
  }

  fn json_schema(
    generator: &mut schemars::SchemaGenerator,
  ) -> schemars::Schema {
    let prefix = format!(
      "#{}/",
      generator
        .settings()
        .definitions_path
        .trim_start_matches('#')
        .trim_end_matches('/')
    );
    for (name, schema) in cloud_init_schema_components(&prefix) {
      generator.definitions_mut().insert(name, schema);
    }
    serde_json::from_value(cloud_init_user_data_schema(&prefix))
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
  /// Maximum vCPU count reserved at boot; omitted uses Cpu and disables CPU growth.
  #[cfg_attr(feature = "utoipa", schema(minimum = 1, maximum = 255))]
  #[cfg_attr(feature = "schemars", schemars(range(min = 1, max = 255)))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub max_cpu: Option<u64>,
  /// Maximum memory in MiB reserved at boot; omitted uses Memory and disables memory growth.
  #[cfg_attr(
    feature = "utoipa",
    schema(minimum = 1, maximum = 8796093022207_u64)
  )]
  #[cfg_attr(
    feature = "schemars",
    schemars(range(min = 1, max = 8796093022207_u64))
  )]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub max_memory: Option<u64>,
  /// Network interface of the vm to setup (default: nanocl0)
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
    serde(rename = "NetworkMode", skip_serializing_if = "Option::is_none")
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
      max_cpu: None,
      max_memory: None,
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

/// Host settings changed by PATCH. Omitted settings retain their current value;
/// explicit null clears optional settings.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct VmHostConfigUpdate {
  #[cfg_attr(feature = "utoipa", schema(nullable = false))]
  #[cfg_attr(feature = "schemars", schemars(with = "u64"))]
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub cpu: Option<u64>,
  #[cfg_attr(feature = "utoipa", schema(nullable = false))]
  #[cfg_attr(feature = "schemars", schemars(with = "u64"))]
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub memory: Option<u64>,
  #[cfg_attr(feature = "utoipa", schema(minimum = 1, maximum = 255))]
  #[cfg_attr(feature = "schemars", schemars(range(min = 1, max = 255)))]
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub max_cpu: Option<Option<u64>>,
  #[cfg_attr(
    feature = "utoipa",
    schema(minimum = 1, maximum = 8796093022207u64)
  )]
  #[cfg_attr(
    feature = "schemars",
    schemars(range(min = 1, max = 8796093022207u64))
  )]
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub max_memory: Option<Option<u64>>,
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub net_iface: Option<Option<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub link_net_iface: Option<Option<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub kvm: Option<Option<bool>>,
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub dns: Option<Option<Vec<String>>>,
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub runtime: Option<Option<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      rename = "NetworkMode",
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub runtime_network: Option<Option<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub host_tun: Option<Option<bool>>,
}

impl VmHostConfigUpdate {
  pub fn apply_to(&self, current: &VmHostConfig) -> VmHostConfig {
    VmHostConfig {
      cpu: self.cpu.unwrap_or(current.cpu),
      memory: self.memory.unwrap_or(current.memory),
      max_cpu: self.max_cpu.unwrap_or(current.max_cpu),
      max_memory: self.max_memory.unwrap_or(current.max_memory),
      net_iface: self
        .net_iface
        .clone()
        .unwrap_or_else(|| current.net_iface.clone()),
      link_net_iface: self
        .link_net_iface
        .clone()
        .unwrap_or_else(|| current.link_net_iface.clone()),
      kvm: self.kvm.unwrap_or(current.kvm),
      dns: self.dns.clone().unwrap_or_else(|| current.dns.clone()),
      runtime: self
        .runtime
        .clone()
        .unwrap_or_else(|| current.runtime.clone()),
      runtime_network: self
        .runtime_network
        .clone()
        .unwrap_or_else(|| current.runtime_network.clone()),
      host_tun: self.host_tun.unwrap_or(current.host_tun),
    }
  }
}

impl From<VmHostConfig> for VmHostConfigUpdate {
  fn from(value: VmHostConfig) -> Self {
    Self {
      cpu: Some(value.cpu),
      memory: Some(value.memory),
      max_cpu: Some(value.max_cpu),
      max_memory: Some(value.max_memory),
      net_iface: Some(value.net_iface),
      link_net_iface: Some(value.link_net_iface),
      kvm: Some(value.kvm),
      dns: Some(value.dns),
      runtime: Some(value.runtime),
      runtime_network: Some(value.runtime_network),
      host_tun: Some(value.host_tun),
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
  /// Enable cloud-init provisioning (default: true).
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
  /// Explicit null restores generated user setup; omission preserves existing data on apply.
  #[cfg_attr(feature = "utoipa", schema(nullable = false))]
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
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
  /// Omitted preserves the current data; null restores generated user setup.
  /// An empty string replaces the data with an empty document.
  /// Stored but ignored when CloudInit is false.
  #[cfg_attr(feature = "utoipa", schema(nullable = false))]
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      deserialize_with = "deserialize_optional_update",
      skip_serializing_if = "Option::is_none"
    )
  )]
  pub cloud_init_user_data: Option<VmCloudInitUserData>,
  /// Enable the authenticated browser console; omitted preserves the setting.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub console: Option<bool>,
  /// Stable MAC identity. If supplied, it must match the existing address.
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
  pub host_config: Option<VmHostConfigUpdate>,
}

impl From<VmSpecPartial> for VmSpecUpdate {
  fn from(spec: VmSpecPartial) -> Self {
    Self {
      name: Some(spec.name),
      hostname: spec.hostname,
      user: spec.user,
      labels: spec.labels,
      host_config: spec.host_config.map(Into::into),
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
  /// Enable cloud-init provisioning (default: true).
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cloud_init: Option<bool>,
  /// Complete cloud-init user-data as raw text or a cloud-config object.
  /// Replaces generated user setup; rendered UTF-8 is limited to 64 KiB.
  /// NanoCL retains its instance ID and DHCP network configuration.
  /// Stored but ignored when CloudInit is false.
  #[cfg_attr(feature = "utoipa", schema(nullable = false))]
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
      host_config: Some(spec.host_config.into()),
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
  fn vm_network_mode_uses_current_json_and_statefile_field() {
    let host = serde_json::json!({
      "Cpu": 2, "Memory": 1024, "NetworkMode": "private-vms"
    });
    let config: VmHostConfig = serde_json::from_value(host.clone()).unwrap();
    assert_eq!(config.runtime_network.as_deref(), Some("private-vms"));
    assert_eq!(serde_json::to_value(config).unwrap(), host);
    let state: crate::statefile::Statefile = serde_yaml::from_str(
      "ApiVersion: v0.19.0\nVirtualMachines:\n- Name: guest\n  Image: /vm.qcow2\n  HostConfig:\n    Cpu: 2\n    Memory: 1024\n    NetworkMode: private-vms\n",
    ).unwrap();
    assert_eq!(
      state.virtual_machines.unwrap()[0]
        .host_config
        .as_ref()
        .unwrap()
        .runtime_network
        .as_deref(),
      Some("private-vms")
    );
    let payload =
      serde_json::json!({"HostConfig": {"NetworkMode": "private-vms"}});
    let patch: VmSpecUpdate = serde_json::from_value(payload.clone()).unwrap();
    assert_eq!(serde_json::to_value(patch).unwrap(), payload);
    let patch: VmSpecUpdate =
      serde_yaml::from_str("HostConfig:\n  NetworkMode: private-vms\n")
        .unwrap();
    assert_eq!(
      patch.host_config.unwrap().runtime_network,
      Some(Some("private-vms".into()))
    );
    assert!(
      serde_json::from_value::<VmHostConfig>(serde_json::json!({
        "Cpu": 2, "Memory": 1024, "RuntimeNetwork": "private-vms"
      }))
      .is_err()
    );
    assert!(
      serde_json::from_value::<VmSpecUpdate>(serde_json::json!({
        "HostConfig": {"RuntimeNetwork": "private-vms"}
      }))
      .is_err()
    );
    assert!(serde_yaml::from_str::<crate::statefile::Statefile>(
      "ApiVersion: v0.19.0\nVirtualMachines:\n- Name: guest\n  Image: /vm.qcow2\n  HostConfig:\n    Cpu: 2\n    Memory: 1024\n    RuntimeNetwork: private-vms\n",
    ).is_err());
  }

  #[test]
  fn vm_host_patch_rejects_null_required_resources() {
    for field in ["Cpu", "Memory"] {
      assert!(
        serde_json::from_value::<VmSpecUpdate>(serde_json::json!({
          "HostConfig": {field: null},
        }))
        .is_err(),
        "{field} cannot be cleared"
      );
      assert!(
        serde_yaml::from_str::<VmSpecUpdate>(&format!(
          "HostConfig:\n  {field}: null\n"
        ))
        .is_err(),
        "{field} cannot be cleared in YAML either"
      );
    }
    for payload in [
      serde_json::json!({}),
      serde_json::json!({"Cpu": 4}),
      serde_json::json!({"Memory": 2048}),
      serde_json::json!({"Cpu": 4, "Memory": 2048}),
    ] {
      let patch: VmHostConfigUpdate =
        serde_json::from_value(payload.clone()).unwrap();
      assert_eq!(serde_json::to_value(patch).unwrap(), payload);
    }
  }

  #[test]
  fn vm_host_patch_is_sparse_and_merges_with_latest_settings() {
    let patch: VmSpecUpdate = serde_json::from_value(serde_json::json!({
      "HostConfig": {"Cpu": 4},
    }))
    .unwrap();
    let latest = VmHostConfig {
      cpu: 2,
      memory: 4096,
      max_cpu: Some(8),
      max_memory: Some(8192),
      runtime: Some("local/runtime:candidate".into()),
      runtime_network: Some("private-vms".into()),
      ..Default::default()
    };
    let mut expected = latest.clone();
    expected.cpu = 4;
    assert_eq!(
      patch.host_config.as_ref().unwrap().apply_to(&latest),
      expected
    );
    assert_eq!(
      serde_json::to_value(patch).unwrap(),
      serde_json::json!({"HostConfig": {"Cpu": 4}})
    );
    let clear: VmHostConfigUpdate = serde_json::from_value(serde_json::json!({
      "MaxCpu": null, "MaxMemory": null, "Runtime": null, "NetworkMode": null,
    }))
    .unwrap();
    let cleared = clear.apply_to(&latest);
    assert_eq!(cleared.cpu, latest.cpu);
    assert_eq!(cleared.memory, latest.memory);
    assert!(cleared.max_cpu.is_none());
    assert!(cleared.max_memory.is_none());
    assert!(cleared.runtime.is_none());
    assert!(cleared.runtime_network.is_none());
    let full = VmHostConfigUpdate::from(VmHostConfig::default());
    let encoded = serde_json::to_value(&full).unwrap();
    assert_eq!(encoded.get("Runtime"), Some(&serde_json::Value::Null));
    let decoded: VmHostConfigUpdate = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.apply_to(&latest), VmHostConfig::default());
  }

  #[test]
  fn vm_cloud_init_null_and_omission_roundtrip() {
    let state: crate::statefile::Statefile = serde_yaml::from_str(
      "ApiVersion: v0.19.0\nVirtualMachines:\n- Name: guest\n  Image: /vm.qcow2\n  CloudInitUserData: null\n",
    ).unwrap();
    let partial = state.virtual_machines.unwrap().remove(0);
    assert_eq!(
      partial.cloud_init_user_data,
      Some(VmCloudInitUserData::Reset)
    );
    let update = VmSpecUpdate::from(partial);
    assert_eq!(
      serde_json::to_value(update)
        .unwrap()
        .get("CloudInitUserData"),
      Some(&serde_json::Value::Null)
    );
    let omitted: VmSpecUpdate =
      serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(omitted.cloud_init_user_data.is_none());
    assert!(
      serde_json::to_value(omitted)
        .unwrap()
        .get("CloudInitUserData")
        .is_none()
    );
  }

  #[test]
  fn vm_host_config_resize_limits_preserve_defaults_and_wire_values() {
    let defaults: VmHostConfig =
      serde_json::from_value(serde_json::json!({"Cpu": 2, "Memory": 1024}))
        .unwrap();
    assert_eq!(defaults.max_cpu, None);
    assert_eq!(defaults.max_memory, None);
    let configured: VmHostConfig = serde_json::from_value(serde_json::json!({"Cpu": 2, "Memory": 1024, "MaxCpu": 8, "MaxMemory": 8192})).unwrap();
    assert_eq!(configured.max_cpu, Some(8));
    assert_eq!(configured.max_memory, Some(8192));
    assert_eq!(
      serde_json::to_value(&configured).unwrap()["MaxMemory"],
      8192
    );
    let yaml: VmSpecPartial = serde_yaml::from_str("Name: guest\nImage: /vm.qcow2\nHostConfig:\n  Cpu: 2\n  Memory: 1024\n  MaxCpu: 8\n  MaxMemory: 8192\n").unwrap();
    assert_eq!(yaml.host_config.unwrap(), configured);
  }

  #[test]
  fn vm_optional_provisioning_and_console_preserve_wire_values() {
    let defaults: VmSpecPartial = serde_json::from_value(serde_json::json!({
      "Name": "guest", "Image": "/vm.qcow2"
    }))
    .unwrap();
    assert_eq!(defaults.cloud_init, None);
    assert_eq!(defaults.cloud_init_user_data, None);
    assert_eq!(defaults.console, None);
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
    assert_eq!(null.cloud_init_user_data, Some(VmCloudInitUserData::Reset));
    assert_eq!(
      serde_json::to_value(null).unwrap()["CloudInitUserData"],
      serde_json::Value::Null
    );
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
  fn vm_cloud_init_openapi_fields_reuse_the_nullable_union() {
    for schema in [
      <VmSpecPartial as utoipa::PartialSchema>::schema(),
      <VmSpecUpdate as utoipa::PartialSchema>::schema(),
      <VmSpec as utoipa::PartialSchema>::schema(),
    ] {
      let schema = serde_json::to_value(schema).unwrap();
      let field = &schema["properties"]["CloudInitUserData"];
      assert_eq!(field["$ref"], "#/components/schemas/VmCloudInitUserData");
      assert!(
        field.get("oneOf").is_none(),
        "the union already accepts null"
      );
      assert!(!schema["required"].as_array().is_some_and(|fields| {
        fields.contains(&serde_json::json!("CloudInitUserData"))
      }));
    }
  }

  #[cfg(any(feature = "utoipa", feature = "schemars"))]
  #[test]
  fn vm_cloud_init_module_tuple_references_the_original_enum() {
    let upstream: serde_json::Value =
      serde_json::from_str(include_str!("../schemas/cloud-init-26.2.json"))
        .unwrap();
    assert_eq!(
      upstream
        .pointer("/$defs/modules_definition/items/oneOf/1/prefixItems/0")
        .unwrap(),
      &serde_json::json!({"enum": {"$ref": "#/$defs/all_modules"}})
    );
    for prefix in ["#/components/schemas/", "#/definitions/"] {
      let components = cloud_init_schema_components(prefix);
      assert_eq!(
        components["CloudInit26_2.modules_definition"]["items"]["oneOf"][1]["prefixItems"]
          [0],
        serde_json::json!({"$ref": format!("{prefix}CloudInit26_2.all_modules")})
      );
      assert_eq!(
        components["CloudInit26_2.all_modules"],
        upstream["$defs"]["all_modules"]
      );
      let names = components["CloudInit26_2.all_modules"]["enum"]
        .as_array()
        .unwrap();
      assert!(names.iter().all(serde_json::Value::is_string));
      assert!(names.contains(&serde_json::json!("growpart")));
      assert!(!names.contains(&serde_json::json!("unknown-probe-module")));
    }
  }

  #[cfg(feature = "utoipa")]
  #[test]
  fn vm_cloud_init_user_data_openapi_preserves_bundled_schema() {
    let schema = serde_json::to_value(
      <VmCloudInitUserData as utoipa::PartialSchema>::schema(),
    )
    .unwrap();
    assert_eq!(schema["oneOf"][0]["type"], "string");
    assert_eq!(schema["oneOf"][0]["maxLength"], 65536);
    assert_eq!(
      schema["oneOf"][1]["$ref"],
      "#/components/schemas/CloudInit26_2"
    );
    assert_eq!(schema["oneOf"].as_array().unwrap().len(), 3);
    assert_eq!(schema["oneOf"][2]["type"], "null");
    let mut components = Vec::new();
    <VmCloudInitUserData as utoipa::ToSchema>::schemas(&mut components);
    let components: BTreeMap<_, _> = components
      .into_iter()
      .map(|(name, schema)| {
        let json = serde_json::to_value(&schema).unwrap();
        // Value's YAML map visitor rejects duplicate keys; JSON maps alone
        // could hide a duplicate type emitted by the flattened keyword map.
        let yaml: serde_yaml::Value =
          serde_yaml::from_str(&serde_yaml::to_string(&schema).unwrap())
            .unwrap();
        assert_eq!(serde_json::to_value(yaml).unwrap(), json, "{name}");
        (name, json)
      })
      .collect();
    let expected = cloud_init_schema_components("#/components/schemas/");
    // Exact serialization keeps keywords not represented by Utoipa's enum,
    // including patternProperties and mixed properties/oneOf constraints.
    assert_eq!(components, expected);
    assert_eq!(components.len(), 77);
    let user = &components["CloudInit26_2.users_groups.user"];
    assert!(user["oneOf"].is_array());
    assert!(user["properties"]["name"].is_object());
    assert_eq!(user["additionalProperties"], false);
    assert!(
      components["CloudInit26_2.users_groups.groups_by_groupname"]
        ["patternProperties"]
        .is_object()
    );
    let document = serde_json::json!({"components": {"schemas": components}});
    assert_local_cloud_init_references(
      &document,
      &document,
      "#/components/schemas/",
    );
  }

  #[cfg(feature = "schemars")]
  #[test]
  fn vm_cloud_init_user_data_statefile_schema_resolves_bundled_definitions() {
    use schemars::generate::SchemaSettings;

    let mut custom = SchemaSettings::draft07();
    custom.definitions_path = "#/components/schemas/".into();
    for (settings, definitions, prefix) in [
      (SchemaSettings::draft07(), "/definitions", "#/definitions/"),
      (SchemaSettings::draft2020_12(), "/$defs", "#/$defs/"),
      (custom, "/components/schemas", "#/components/schemas/"),
    ] {
      let dialect = settings.meta_schema.clone().unwrap();
      let schema = settings
        .into_generator()
        .into_root_schema_for::<crate::statefile::Statefile>()
        .to_value();
      assert_eq!(schema["$schema"], dialect.as_ref());
      let definitions = schema.pointer(definitions).unwrap();
      let data = &definitions["VmCloudInitUserData"];
      assert_eq!(data["oneOf"][0]["type"], "string");
      assert_eq!(data["oneOf"][0]["maxLength"], 65536);
      assert_eq!(data["oneOf"][1]["$ref"], format!("{prefix}CloudInit26_2"));
      assert_eq!(data["oneOf"].as_array().unwrap().len(), 3);
      assert_eq!(data["oneOf"][2]["type"], "null");
      assert_local_cloud_init_references(&schema, &schema, prefix);
      let root = &definitions["CloudInit26_2"];
      assert_eq!(root["type"], "object");
      assert_eq!(root["additionalProperties"], false);
      assert!(root["allOf"].is_array());
      assert!(root["properties"]["users"].is_object());
      let user = &definitions["CloudInit26_2.users_groups.user"];
      assert!(user["oneOf"].is_array());
      assert_eq!(user["properties"]["name"]["type"], "string");
    }
  }

  #[cfg(feature = "utoipa")]
  #[test]
  fn vm_host_config_openapi_bounds_boot_limits() {
    let schema =
      serde_json::to_value(<VmHostConfig as utoipa::PartialSchema>::schema())
        .unwrap();
    assert_boot_limit_schema(&schema);
    let update = serde_json::to_value(
      <VmHostConfigUpdate as utoipa::PartialSchema>::schema(),
    )
    .unwrap();
    assert_boot_limit_schema(&update);
    assert_required_resource_patch_schema(&update);
  }

  #[cfg(feature = "schemars")]
  #[test]
  fn vm_host_config_statefile_schema_bounds_boot_limits() {
    let schema = schemars::generate::SchemaSettings::draft07()
      .into_generator()
      .into_root_schema_for::<VmHostConfig>()
      .to_value();
    assert_boot_limit_schema(&schema);
    let update = schemars::generate::SchemaSettings::draft07()
      .into_generator()
      .into_root_schema_for::<VmHostConfigUpdate>()
      .to_value();
    assert_boot_limit_schema(&update);
    assert_required_resource_patch_schema(&update);
  }

  #[cfg(any(feature = "utoipa", feature = "schemars"))]
  fn assert_required_resource_patch_schema(schema: &serde_json::Value) {
    for field in ["Cpu", "Memory"] {
      assert_eq!(schema["properties"][field]["type"], "integer", "{field}");
      assert!(
        !schema["required"]
          .as_array()
          .is_some_and(|fields| { fields.contains(&serde_json::json!(field)) })
      );
    }
  }

  #[cfg(any(feature = "utoipa", feature = "schemars"))]
  fn assert_boot_limit_schema(schema: &serde_json::Value) {
    for (field, maximum) in [("MaxCpu", 255_u64), ("MaxMemory", 8796093022207)]
    {
      let property = &schema["properties"][field];
      assert_eq!(property["minimum"].as_f64(), Some(1.0), "{field}");
      assert_eq!(
        property["maximum"].as_f64(),
        Some(maximum as f64),
        "{field}"
      );
    }
  }

  #[cfg(any(feature = "utoipa", feature = "schemars"))]
  fn assert_local_cloud_init_references(
    document: &serde_json::Value,
    value: &serde_json::Value,
    prefix: &str,
  ) {
    match value {
      serde_json::Value::Array(items) => {
        for item in items {
          assert_local_cloud_init_references(document, item, prefix);
        }
      }
      serde_json::Value::Object(object) => {
        if let Some(reference) = object.get("$ref") {
          let reference = reference.as_str().unwrap();
          assert!(reference.starts_with(prefix), "{reference}");
          assert!(document.pointer(&reference[1..]).is_some(), "{reference}");
        }
        for child in object.values() {
          assert_local_cloud_init_references(document, child, prefix);
        }
      }
      _ => {}
    }
  }
}
