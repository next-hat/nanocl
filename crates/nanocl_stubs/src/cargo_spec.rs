#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(any(feature = "schemars", feature = "utoipa"))]
use std::collections::BTreeSet;
use std::collections::HashMap;
#[cfg(feature = "serde")]
use std::collections::{BTreeMap, HashSet};

pub use bollard_next::{
  container::Config,
  models::{HealthConfig, HostConfig, PortBinding, PortMap},
};

#[cfg(feature = "utoipa")]
use super::generic::Any;

use crate::generic::ImagePullPolicy;

/// Docker network mode used by a Cargo.
///
/// Docker accepts any non-blank network name in addition to its built-in
/// modes. Nanocl reserves `container:<id>` because users must not join a
/// container-owned network namespace directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct CargoNetworkMode(String);

impl CargoNetworkMode {
  /// Create a Cargo network mode from a Docker network name or built-in mode.
  pub fn new(value: impl Into<String>) -> Result<Self, CargoNetworkModeError> {
    let value = value.into();
    if value.trim().is_empty() {
      return Err(CargoNetworkModeError::Empty);
    }
    if value.starts_with("container:") {
      return Err(CargoNetworkModeError::ContainerNamespace);
    }
    Ok(Self(value))
  }

  /// Return the Docker network mode as scalar text.
  pub fn as_str(&self) -> &str {
    &self.0
  }

  /// Consume the mode and return its scalar representation.
  pub fn into_inner(self) -> String {
    self.0
  }
}

impl std::fmt::Display for CargoNetworkMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(self.as_str())
  }
}

impl std::str::FromStr for CargoNetworkMode {
  type Err = CargoNetworkModeError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl TryFrom<String> for CargoNetworkMode {
  type Error = CargoNetworkModeError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    Self::new(value)
  }
}

impl AsRef<str> for CargoNetworkMode {
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for CargoNetworkMode {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let value = String::deserialize(deserializer)?;
    Self::new(value).map_err(serde::de::Error::custom)
  }
}

/// Error returned for a Cargo network mode Nanocl cannot safely own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoNetworkModeError {
  /// Docker network names must contain at least one non-whitespace character.
  Empty,
  /// Joining another container's network namespace is reserved for Nanocl.
  ContainerNamespace,
}

impl std::fmt::Display for CargoNetworkModeError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Empty => f.write_str("CargoSpec.NetworkMode cannot be blank"),
      Self::ContainerNamespace => f.write_str(
        "CargoSpec.NetworkMode cannot use a container namespace reference",
      ),
    }
  }
}

impl std::error::Error for CargoNetworkModeError {}

/// Strict Serde adapters for generated Bollard models.
///
/// Bollard's generated models intentionally ignore unknown fields. These
/// adapters retain those exact models while turning every ignored nested key
/// into a deserialization error.
#[cfg(feature = "serde")]
pub mod strict_bollard {
  use super::{Config, PortMap};

  fn push_path(path: &serde_ignored::Path<'_>, segments: &mut Vec<String>) {
    match path {
      serde_ignored::Path::Root => {}
      serde_ignored::Path::Seq { parent, index } => {
        push_path(parent, segments);
        segments.push(index.to_string());
      }
      serde_ignored::Path::Map { parent, key } => {
        push_path(parent, segments);
        segments.push(key.clone());
      }
      serde_ignored::Path::Some { parent }
      | serde_ignored::Path::NewtypeStruct { parent }
      | serde_ignored::Path::NewtypeVariant { parent } => {
        push_path(parent, segments);
      }
    }
  }

  fn render_path(path: &serde_ignored::Path<'_>) -> String {
    let mut segments = Vec::new();
    push_path(path, &mut segments);
    segments.join(".")
  }

  fn deserialize_strict<'de, D, T>(
    deserializer: D,
    root: &str,
  ) -> Result<T, D::Error>
  where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
  {
    let mut unknown = Vec::new();
    let value = serde_ignored::deserialize(deserializer, |path| {
      let path = render_path(&path);
      unknown.push(if path.is_empty() {
        root.to_owned()
      } else {
        format!("{root}.{path}")
      });
    })?;
    unknown.sort_unstable();
    unknown.dedup();

    if unknown.is_empty() {
      Ok(value)
    } else {
      Err(serde::de::Error::custom(format!(
        "unknown Bollard field(s): {}",
        unknown.join(", ")
      )))
    }
  }

  /// Deserialize a Bollard container [`Config`] without ignored fields.
  pub fn deserialize_config<'de, D>(deserializer: D) -> Result<Config, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserialize_strict(deserializer, "ContainerSpec")
  }

  /// Deserialize a Bollard [`PortMap`] without ignored binding fields.
  pub fn deserialize_port_map<'de, D>(
    deserializer: D,
  ) -> Result<PortMap, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserialize_strict(deserializer, "PortBindings")
  }

  /// Deserialize an optional Bollard [`PortMap`] without ignored binding
  /// fields.
  pub fn deserialize_optional_port_map<'de, D>(
    deserializer: D,
  ) -> Result<Option<PortMap>, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserialize_strict(deserializer, "PortBindings")
  }
}

#[cfg(feature = "serde")]
fn default_container_essential() -> bool {
  true
}

/// Field names owned by Nanocl in the flattened [`ContainerSpec`] mapping.
///
/// Every other key is delegated to the pinned Bollard [`Config`]. A Bollard
/// upgrade that introduces one of these names must be reviewed explicitly.
const NANOCL_CONTAINER_FIELDS: [&str; 5] = [
  "Name",
  "Essential",
  "Secrets",
  "ImagePullSecret",
  "ImagePullPolicy",
];

#[cfg(feature = "schemars")]
fn collect_schema_references(
  schema: &serde_json::Value,
  reference_prefix: &str,
  references: &mut BTreeSet<String>,
) {
  match schema {
    serde_json::Value::Array(values) => {
      for value in values {
        collect_schema_references(value, reference_prefix, references);
      }
    }
    serde_json::Value::Object(object) => {
      if let Some(reference) =
        object.get("$ref").and_then(serde_json::Value::as_str)
        && let Some(name) = reference.strip_prefix(reference_prefix)
      {
        references.insert(name.to_owned());
      }
      for value in object.values() {
        collect_schema_references(value, reference_prefix, references);
      }
    }
    _ => {}
  }
}

#[cfg(any(feature = "schemars", feature = "utoipa"))]
fn strict_container_component_name(name: &str) -> String {
  format!("ContainerSpec{name}")
}

/// Close generated struct objects while retaining typed map semantics, and
/// retarget their references to ContainerSpec-private components.
#[cfg(feature = "schemars")]
fn close_and_retarget_container_schema(
  schema: &mut serde_json::Value,
  reference_prefix: &str,
  strict_components: &BTreeSet<String>,
) {
  match schema {
    serde_json::Value::Array(values) => {
      for value in values {
        close_and_retarget_container_schema(
          value,
          reference_prefix,
          strict_components,
        );
      }
    }
    serde_json::Value::Object(object) => {
      if let Some(serde_json::Value::String(reference)) = object.get_mut("$ref")
      {
        let strict_name = reference
          .strip_prefix(reference_prefix)
          .filter(|name| strict_components.contains(*name))
          .map(strict_container_component_name);
        if let Some(strict_name) = strict_name {
          *reference = format!("{reference_prefix}{strict_name}");
        }
      }

      for value in object.values_mut() {
        close_and_retarget_container_schema(
          value,
          reference_prefix,
          strict_components,
        );
      }

      if object
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .is_some()
        && !object.contains_key("additionalProperties")
      {
        object.insert(
          "additionalProperties".to_owned(),
          serde_json::Value::Bool(false),
        );
      }
    }
    _ => {}
  }
}

/// Declaration of one named container in a multi-container Cargo.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ContainerSpec {
  /// Name unique within the Cargo declaration.
  pub name: String,
  /// Whether failure of this container makes the Cargo replica unhealthy.
  #[cfg_attr(
    feature = "serde",
    serde(default = "default_container_essential")
  )]
  pub essential: bool,
  /// Secrets injected into only this container.
  #[cfg_attr(feature = "serde", serde(default))]
  pub secrets: Vec<String>,
  /// Secret used to pull this container's image.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Policy used to pull this container's image.
  #[cfg_attr(feature = "serde", serde(default))]
  pub image_pull_policy: ImagePullPolicy,
  /// Raw authored Docker container configuration.
  ///
  /// This remains distinct in Rust but is flattened beside Nanocl-owned fields
  /// in every public serialization format.
  #[cfg_attr(feature = "serde", serde(flatten))]
  pub container_config: Config,
}

#[cfg(feature = "schemars")]
fn strict_schemars_config_schema(
  generator: &mut schemars::r#gen::SchemaGenerator,
) -> schemars::schema::Schema {
  use schemars::JsonSchema;

  let config_schema = Config::json_schema(generator);
  let reference_prefix = generator.settings().definitions_path.clone();
  let config_value = serde_json::to_value(&config_schema)
    .expect("Bollard Config must serialize as a Schemars schema");

  let mut reachable = BTreeSet::new();
  collect_schema_references(&config_value, &reference_prefix, &mut reachable);
  let mut pending = reachable.iter().cloned().collect::<Vec<_>>();
  while let Some(name) = pending.pop() {
    let Some(schema) = generator.definitions().get(&name) else {
      continue;
    };
    let schema = serde_json::to_value(schema)
      .expect("Bollard definition must serialize as a Schemars schema");
    let mut nested = BTreeSet::new();
    collect_schema_references(&schema, &reference_prefix, &mut nested);
    for name in nested {
      if reachable.insert(name.clone()) {
        pending.push(name);
      }
    }
  }

  let strict_components = reachable
    .iter()
    .filter(|name| generator.definitions().contains_key(*name))
    .cloned()
    .collect::<BTreeSet<_>>();

  let strict_definitions = strict_components
    .iter()
    .map(|name| {
      let mut schema = serde_json::to_value(
        generator
          .definitions()
          .get(name)
          .expect("reachable Bollard definition must exist"),
      )
      .expect("Bollard definition must serialize as a Schemars schema");
      close_and_retarget_container_schema(
        &mut schema,
        &reference_prefix,
        &strict_components,
      );
      let schema = serde_json::from_value(schema)
        .expect("strict Bollard definition must remain a Schemars schema");
      (strict_container_component_name(name), schema)
    })
    .collect::<Vec<_>>();

  for (name, schema) in strict_definitions {
    if let Some(existing) = generator.definitions().get(&name) {
      assert_eq!(
        existing, &schema,
        "Schemars component {name} collided with ContainerSpec's strict Bollard schema"
      );
    } else {
      generator.definitions_mut().insert(name, schema);
    }
  }

  let mut config_value = config_value;
  close_and_retarget_container_schema(
    &mut config_value,
    &reference_prefix,
    &strict_components,
  );
  serde_json::from_value(config_value)
    .expect("strict Bollard Config must remain a Schemars schema")
}

#[cfg(feature = "schemars")]
fn schemars_container_property<T: schemars::JsonSchema>(
  generator: &mut schemars::r#gen::SchemaGenerator,
  description: &str,
  default: Option<serde_json::Value>,
) -> schemars::schema::Schema {
  let mut schema = generator.subschema_for::<T>().into_object();
  schema.metadata().description = Some(description.to_owned());
  schema.metadata().default = default;
  schema.into()
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ContainerSpec {
  fn schema_name() -> String {
    "ContainerSpec".to_owned()
  }

  fn schema_id() -> std::borrow::Cow<'static, str> {
    std::borrow::Cow::Borrowed(concat!(module_path!(), "::ContainerSpec"))
  }

  fn json_schema(
    generator: &mut schemars::r#gen::SchemaGenerator,
  ) -> schemars::schema::Schema {
    let mut schema = strict_schemars_config_schema(generator).into_object();
    schema.metadata().description = Some(
      "Declaration of one named container in a multi-container Cargo."
        .to_owned(),
    );
    let object = schema.object();
    object.additional_properties = Some(Box::new(false.into()));

    let fields = [
      (
        NANOCL_CONTAINER_FIELDS[0],
        schemars_container_property::<String>(
          generator,
          "Name unique within the Cargo declaration.",
          None,
        ),
      ),
      (
        NANOCL_CONTAINER_FIELDS[1],
        schemars_container_property::<bool>(
          generator,
          "Whether failure of this container makes the Cargo replica unhealthy.",
          Some(serde_json::Value::Bool(true)),
        ),
      ),
      (
        NANOCL_CONTAINER_FIELDS[2],
        schemars_container_property::<Vec<String>>(
          generator,
          "Secrets injected into only this container.",
          Some(serde_json::json!([])),
        ),
      ),
      (
        NANOCL_CONTAINER_FIELDS[3],
        schemars_container_property::<Option<String>>(
          generator,
          "Secret used to pull this container's image.",
          None,
        ),
      ),
      (
        NANOCL_CONTAINER_FIELDS[4],
        schemars_container_property::<ImagePullPolicy>(
          generator,
          "Policy used to pull this container's image.",
          Some(serde_json::Value::String("IfNotPresent".to_owned())),
        ),
      ),
    ];
    for (name, field_schema) in fields {
      assert!(
        object
          .properties
          .insert(name.to_owned(), field_schema)
          .is_none(),
        "Bollard Config field {name:?} collides with ContainerSpec's Nanocl namespace"
      );
    }
    object
      .required
      .insert(NANOCL_CONTAINER_FIELDS[0].to_owned());
    schema.into()
  }
}

#[cfg(feature = "utoipa")]
type UtoipaSchema = utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>;

#[cfg(feature = "utoipa")]
fn utoipa_config_references() -> Vec<(String, UtoipaSchema)> {
  let mut references = Vec::new();
  <Config as utoipa::ToSchema>::schemas(&mut references);
  references
}

#[cfg(feature = "utoipa")]
fn strict_utoipa_component_names(
  references: &[(String, UtoipaSchema)],
) -> BTreeSet<String> {
  references.iter().map(|(name, _)| name.clone()).collect()
}

#[cfg(feature = "utoipa")]
fn retarget_utoipa_reference(
  reference: &mut utoipa::openapi::Ref,
  strict_components: &BTreeSet<String>,
) {
  const REFERENCE_PREFIX: &str = "#/components/schemas/";
  let strict_name = reference
    .ref_location
    .strip_prefix(REFERENCE_PREFIX)
    .filter(|name| strict_components.contains(*name))
    .map(strict_container_component_name);
  if let Some(strict_name) = strict_name {
    reference.ref_location = format!("{REFERENCE_PREFIX}{strict_name}");
  }
}

#[cfg(feature = "utoipa")]
fn close_and_retarget_utoipa_schema(
  schema: &mut UtoipaSchema,
  strict_components: &BTreeSet<String>,
) {
  use utoipa::openapi::schema::{AdditionalProperties, ArrayItems, Schema};

  match schema {
    utoipa::openapi::RefOr::Ref(reference) => {
      retarget_utoipa_reference(reference, strict_components);
    }
    utoipa::openapi::RefOr::T(schema) => match schema {
      Schema::Object(schema) => {
        for property in schema.properties.values_mut() {
          close_and_retarget_utoipa_schema(property, strict_components);
        }
        if let Some(additional_properties) =
          schema.additional_properties.as_deref_mut()
          && let AdditionalProperties::RefOr(additional_properties) =
            additional_properties
        {
          close_and_retarget_utoipa_schema(
            additional_properties,
            strict_components,
          );
        }
        if let Some(property_names) = schema.property_names.as_deref_mut() {
          let mut property_names =
            utoipa::openapi::RefOr::T(property_names.clone());
          close_and_retarget_utoipa_schema(
            &mut property_names,
            strict_components,
          );
          if let utoipa::openapi::RefOr::T(property_names) = property_names {
            **schema.property_names.as_mut().unwrap() = property_names;
          }
        }
        if !schema.properties.is_empty()
          && schema.additional_properties.is_none()
        {
          schema.additional_properties =
            Some(Box::new(AdditionalProperties::FreeForm(false)));
        }
      }
      Schema::Array(schema) => {
        if let ArrayItems::RefOrSchema(items) = &mut schema.items {
          close_and_retarget_utoipa_schema(items, strict_components);
        }
        for item in &mut schema.prefix_items {
          let mut item_schema = utoipa::openapi::RefOr::T(item.clone());
          close_and_retarget_utoipa_schema(&mut item_schema, strict_components);
          if let utoipa::openapi::RefOr::T(item_schema) = item_schema {
            *item = item_schema;
          }
        }
      }
      Schema::OneOf(schema) => {
        for item in &mut schema.items {
          close_and_retarget_utoipa_schema(item, strict_components);
        }
      }
      Schema::AllOf(schema) => {
        for item in &mut schema.items {
          close_and_retarget_utoipa_schema(item, strict_components);
        }
      }
      Schema::AnyOf(schema) => {
        for item in &mut schema.items {
          close_and_retarget_utoipa_schema(item, strict_components);
        }
      }
      _ => {}
    },
  }
}

#[cfg(feature = "utoipa")]
fn strict_utoipa_schema(
  schema: &UtoipaSchema,
  strict_components: &BTreeSet<String>,
) -> UtoipaSchema {
  let mut schema = schema.clone();
  close_and_retarget_utoipa_schema(&mut schema, strict_components);
  schema
}

#[cfg(feature = "utoipa")]
fn annotate_utoipa_schema(
  schema: UtoipaSchema,
  description: &str,
  default: Option<serde_json::Value>,
) -> UtoipaSchema {
  use utoipa::openapi::schema::{AllOf, Schema};

  match schema {
    utoipa::openapi::RefOr::Ref(reference) => {
      let mut all_of = AllOf::new();
      all_of.items.push(reference.into());
      all_of.description = Some(description.to_owned());
      all_of.default = default;
      Schema::AllOf(all_of).into()
    }
    utoipa::openapi::RefOr::T(mut schema) => {
      match &mut schema {
        Schema::Array(schema) => {
          schema.description = Some(description.to_owned());
          schema.default = default;
        }
        Schema::Object(schema) => {
          schema.description = Some(description.to_owned());
          schema.default = default;
        }
        Schema::OneOf(schema) => {
          schema.description = Some(description.to_owned());
          schema.default = default;
        }
        Schema::AllOf(schema) => {
          schema.description = Some(description.to_owned());
          schema.default = default;
        }
        Schema::AnyOf(schema) => {
          schema.description = Some(description.to_owned());
          schema.default = default;
        }
        _ => {}
      }
      schema.into()
    }
  }
}

#[cfg(feature = "utoipa")]
fn utoipa_container_spec_schema() -> UtoipaSchema {
  use utoipa::PartialSchema;
  use utoipa::openapi::schema::{AdditionalProperties, Schema};

  let references = utoipa_config_references();
  let strict_components = strict_utoipa_component_names(&references);
  let config = <Config as utoipa::PartialSchema>::schema();
  let config = strict_utoipa_schema(&config, &strict_components);
  let utoipa::openapi::RefOr::T(Schema::Object(mut schema)) = config else {
    panic!("pinned Bollard Config must generate an inline Utoipa object");
  };

  schema.description = Some(
    "Declaration of one named container in a multi-container Cargo.".to_owned(),
  );
  schema.additional_properties =
    Some(Box::new(AdditionalProperties::FreeForm(false)));
  let fields = [
    (
      NANOCL_CONTAINER_FIELDS[0],
      annotate_utoipa_schema(
        String::schema(),
        "Name unique within the Cargo declaration.",
        None,
      ),
    ),
    (
      NANOCL_CONTAINER_FIELDS[1],
      annotate_utoipa_schema(
        bool::schema(),
        "Whether failure of this container makes the Cargo replica unhealthy.",
        Some(serde_json::Value::Bool(true)),
      ),
    ),
    (
      NANOCL_CONTAINER_FIELDS[2],
      annotate_utoipa_schema(
        Vec::<String>::schema(),
        "Secrets injected into only this container.",
        Some(serde_json::json!([])),
      ),
    ),
    (
      NANOCL_CONTAINER_FIELDS[3],
      annotate_utoipa_schema(
        Option::<String>::schema(),
        "Secret used to pull this container's image.",
        None,
      ),
    ),
    (
      NANOCL_CONTAINER_FIELDS[4],
      annotate_utoipa_schema(
        ImagePullPolicy::schema(),
        "Policy used to pull this container's image.",
        Some(serde_json::Value::String("IfNotPresent".to_owned())),
      ),
    ),
  ];
  for (name, field_schema) in fields {
    assert!(
      schema
        .properties
        .insert(name.to_owned(), field_schema)
        .is_none(),
      "Bollard Config field {name:?} collides with ContainerSpec's Nanocl namespace"
    );
  }
  schema.required.push(NANOCL_CONTAINER_FIELDS[0].to_owned());
  Schema::Object(schema).into()
}

#[cfg(feature = "utoipa")]
impl utoipa::__dev::ComposeSchema for ContainerSpec {
  fn compose(_: Vec<UtoipaSchema>) -> UtoipaSchema {
    utoipa_container_spec_schema()
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for ContainerSpec {
  fn schemas(schemas: &mut Vec<(String, UtoipaSchema)>) {
    let references = utoipa_config_references();
    let strict_components = strict_utoipa_component_names(&references);
    for (name, schema) in references {
      schemas.push((name.clone(), schema.clone()));
      if strict_components.contains(&name) {
        schemas.push((
          strict_container_component_name(&name),
          strict_utoipa_schema(&schema, &strict_components),
        ));
      }
    }
  }
}

#[cfg(feature = "serde")]
struct ContainerSpecVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for ContainerSpecVisitor {
  type Value = ContainerSpec;

  fn expecting(
    &self,
    formatter: &mut std::fmt::Formatter<'_>,
  ) -> std::fmt::Result {
    formatter.write_str("a flattened ContainerSpec mapping")
  }

  fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
  where
    A: serde::de::MapAccess<'de>,
  {
    use serde::de::Error;

    let mut seen = HashSet::new();
    let mut name = None;
    let mut essential = None;
    let mut secrets = None;
    let mut image_pull_secret = None;
    let mut image_pull_policy = None;
    let mut bollard_fields = BTreeMap::new();

    while let Some(key) = map.next_key::<String>()? {
      let owned_field = NANOCL_CONTAINER_FIELDS
        .iter()
        .position(|field| *field == key);

      if !seen.insert(key.clone()) {
        return match owned_field {
          Some(index) => {
            Err(A::Error::duplicate_field(NANOCL_CONTAINER_FIELDS[index]))
          }
          None => {
            Err(A::Error::custom(format!("duplicate Bollard field `{key}`")))
          }
        };
      }

      match owned_field {
        Some(0) => name = Some(map.next_value()?),
        Some(1) => essential = Some(map.next_value()?),
        Some(2) => secrets = Some(map.next_value()?),
        Some(3) => image_pull_secret = Some(map.next_value()?),
        Some(4) => image_pull_policy = Some(map.next_value()?),
        Some(_) => unreachable!("Nanocl ContainerSpec field table changed"),
        None => {
          bollard_fields.insert(
            serde_value::Value::String(key),
            map.next_value::<serde_value::Value>()?,
          );
        }
      }
    }

    let container_config = strict_bollard::deserialize_config(
      serde_value::Value::Map(bollard_fields),
    )
    .map_err(A::Error::custom)?;

    Ok(ContainerSpec {
      name: name.ok_or_else(|| A::Error::missing_field("Name"))?,
      essential: essential.unwrap_or_else(default_container_essential),
      secrets: secrets.unwrap_or_default(),
      image_pull_secret: image_pull_secret.unwrap_or_default(),
      image_pull_policy: image_pull_policy.unwrap_or_default(),
      container_config,
    })
  }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for ContainerSpec {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserializer.deserialize_map(ContainerSpecVisitor)
  }
}

/// Pure validation error for a [`ContainerSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerSpecValidationError {
  /// The required container name was empty.
  EmptyName,
  /// The Bollard container config did not declare an image.
  MissingImage,
  /// The Bollard container config declared an empty image.
  EmptyImage,
  /// The raw Bollard network mode was not omitted, `host`, or `none`.
  InvalidNetworkMode,
  /// A field reserved for the future Cargo compiler was present.
  ForbiddenField(&'static str),
  /// A raw reference to another container-owned namespace was present.
  ContainerNamespaceReference(&'static str),
}

impl std::fmt::Display for ContainerSpecValidationError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::EmptyName => f.write_str("ContainerSpec.Name cannot be empty"),
      Self::MissingImage => f.write_str("ContainerSpec.Image is required"),
      Self::EmptyImage => f.write_str("ContainerSpec.Image cannot be empty"),
      Self::InvalidNetworkMode => f.write_str(
        "ContainerSpec.HostConfig.NetworkMode must be omitted, \"host\", or \"none\"",
      ),
      Self::ForbiddenField(field) => {
        write!(f, "ContainerSpec.{field} is owned by the Cargo compiler")
      }
      Self::ContainerNamespaceReference(field) => write!(
        f,
        "ContainerSpec.{field} cannot reference another container namespace"
      ),
    }
  }
}

impl std::error::Error for ContainerSpecValidationError {}

impl ContainerSpec {
  /// Validate the declaration without consulting Docker or daemon state.
  pub fn validate(&self) -> Result<(), ContainerSpecValidationError> {
    if self.name.is_empty() {
      return Err(ContainerSpecValidationError::EmptyName);
    }

    match self.container_config.image.as_deref() {
      None => return Err(ContainerSpecValidationError::MissingImage),
      Some("") => return Err(ContainerSpecValidationError::EmptyImage),
      Some(_) => {}
    }

    if self.container_config.networking_config.is_some() {
      return Err(ContainerSpecValidationError::ForbiddenField(
        "NetworkingConfig",
      ));
    }
    if self.container_config.network_disabled.is_some() {
      return Err(ContainerSpecValidationError::ForbiddenField(
        "NetworkDisabled",
      ));
    }

    let Some(host_config) = self.container_config.host_config.as_ref() else {
      return Ok(());
    };

    if host_config
      .network_mode
      .as_deref()
      .is_some_and(|mode| !matches!(mode, "host" | "none"))
    {
      return Err(ContainerSpecValidationError::InvalidNetworkMode);
    }

    for (present, field) in [
      (
        host_config.port_bindings.is_some(),
        "HostConfig.PortBindings",
      ),
      (
        host_config.publish_all_ports.is_some(),
        "HostConfig.PublishAllPorts",
      ),
      (host_config.auto_remove.is_some(), "HostConfig.AutoRemove"),
    ] {
      if present {
        return Err(ContainerSpecValidationError::ForbiddenField(field));
      }
    }

    for (mode, field) in [
      (host_config.ipc_mode.as_deref(), "HostConfig.IpcMode"),
      (host_config.pid_mode.as_deref(), "HostConfig.PidMode"),
      // Docker's CgroupSpec uses container:<id> to select another
      // container's cgroup even though CgroupnsMode itself is a closed enum.
      (host_config.cgroup.as_deref(), "HostConfig.Cgroup"),
      (host_config.uts_mode.as_deref(), "HostConfig.UTSMode"),
      (host_config.userns_mode.as_deref(), "HostConfig.UsernsMode"),
    ] {
      if mode.is_some_and(|mode| mode.starts_with("container:")) {
        return Err(ContainerSpecValidationError::ContainerNamespaceReference(
          field,
        ));
      }
    }

    Ok(())
  }
}

/// Enum to define the strategy to place the cargo on the nodes
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub enum CargoPlacementStrategy {
  Distinct,
  LeastLoaded,
  Balanced,
  UserDefined(String),
}

/// Resource requirements for the cargo
///
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoResourceRequirementSpec {
  /// CPU requirement in number of cores
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cpu_cores: Option<usize>,
  /// Maximum allowed average CPU utilization on a target node (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cpu_utilization_cap: Option<f32>,
  /// Memory requirement in bytes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub memory_bytes: Option<usize>,
  /// Maximum allowed average Memory utilization on a target node (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub memory_utilization_cap: Option<f32>,
  /// Optional weight for CPU in balanced placement (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cpu_weight: Option<f32>,
  /// Optional weight for Memory in balanced placement (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub memory_weight: Option<f32>,
  /// Storage requirement in bytes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub storage_bytes: Option<usize>,
}

/// Generic constraint operator for node selection
/// Allows advanced matching beyond simple selectors
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ConstraintOp {
  Eq,
  Ne,
}

/// Arbitrary node constraint expressed as key/operator/value
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct NodeConstraint {
  /// Constraint key (eg: "region", "label", "group", etc.)
  pub key: String,
  /// Constraint operator (Eq/Ne)
  pub operator: ConstraintOp,
  /// Constraint value
  pub value: String,
}

/// Way to select specific or preferred nodes to place the cargo
///
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoPlacementSelectorSpec {
  /// Select by labels
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<Vec<String>>,
  /// Select by regions
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub regions: Option<Vec<String>>,
  /// Select by groups
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub groups: Option<Vec<String>>,
  /// Select by cargoes running on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cargoes: Option<Vec<String>>,
  /// Select by vms running on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub vms: Option<Vec<String>>,
  /// Optional advanced constraints (key/operator/value)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub constraints: Option<Vec<NodeConstraint>>,
}

/// Structure to control the placement of the cargo
/// It can be used to define on which nodes the cargo will be deployed
///
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoPlacementSpec {
  /// Strategy to use to place the cargo on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub strategy: Option<CargoPlacementStrategy>,
  /// Regions to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub regions: Option<Vec<String>>,
  /// requirements to select nodes to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub require: Option<CargoPlacementSelectorSpec>,
  /// preferences to select nodes to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub prefer: Option<CargoPlacementSelectorSpec>,
}

fn default_cargo_replicas() -> usize {
  1
}

const CARGO_SANDBOX_CONTAINER_NAME: &str = "_sandbox";

/// Desired declaration used to create or apply a Cargo.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoSpec {
  /// Name of the cargo
  pub name: String,
  /// Metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Number of durable Cargo replicas.
  #[cfg_attr(feature = "serde", serde(default = "default_cargo_replicas"))]
  #[cfg_attr(
    feature = "utoipa",
    schema(default = default_cargo_replicas, minimum = 1)
  )]
  #[cfg_attr(feature = "schemars", schemars(range(min = 1)))]
  pub replicas: usize,
  /// Cargo-owned network shared by the replica's containers.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub network_mode: Option<CargoNetworkMode>,
  /// Cargo-owned host port bindings.
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      skip_serializing_if = "Option::is_none",
      deserialize_with = "strict_bollard::deserialize_optional_port_map"
    )
  )]
  pub port_bindings: Option<PortMap>,
  /// Secrets inherited by every regular and init container.
  #[cfg_attr(feature = "serde", serde(default))]
  pub secrets: Vec<String>,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  /// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
  /// Init containers executed in declaration order before application
  /// containers.
  #[cfg_attr(feature = "serde", serde(default))]
  pub init_containers: Vec<ContainerSpec>,
  /// Application containers in each Cargo replica.
  pub containers: Vec<ContainerSpec>,
}

impl Default for CargoSpec {
  fn default() -> Self {
    Self {
      name: String::new(),
      metadata: None,
      replicas: default_cargo_replicas(),
      network_mode: None,
      port_bindings: None,
      secrets: Vec::new(),
      placement: None,
      resource_requirement: None,
      init_containers: Vec::new(),
      containers: Vec::new(),
    }
  }
}

/// Pure semantic validation error for a [`CargoSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoSpecValidationError {
  /// The required Cargo name was empty.
  EmptyName,
  /// A Cargo must declare at least one durable replica.
  InvalidReplicaCount,
  /// A Cargo must declare at least one application container.
  EmptyContainers,
  /// One named application or init container was invalid.
  InvalidContainer {
    path: String,
    source: ContainerSpecValidationError,
  },
  /// A container used the logical name reserved for the Cargo sandbox.
  ReservedContainerName { path: String, name: String },
  /// Container names are unique across both application and init containers.
  DuplicateContainerName {
    name: String,
    first_path: String,
    duplicate_path: String,
  },
  /// At least one application container must be essential.
  MissingEssentialContainer,
  /// Init containers are always essential.
  NonEssentialInitContainer(String),
}

impl std::fmt::Display for CargoSpecValidationError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::EmptyName => f.write_str("CargoSpec.Name cannot be empty"),
      Self::InvalidReplicaCount => {
        f.write_str("CargoSpec.Replicas must be greater than or equal to 1")
      }
      Self::EmptyContainers => {
        f.write_str("CargoSpec.Containers must contain at least one container")
      }
      Self::InvalidContainer { path, source } => {
        write!(f, "CargoSpec.{path}: {source}")
      }
      Self::ReservedContainerName { path, .. } => {
        write!(f, "CargoSpec.{path}.Name is reserved by Nanocl")
      }
      Self::DuplicateContainerName {
        name,
        first_path,
        duplicate_path,
      } => write!(
        f,
        "CargoSpec container name {name:?} is duplicated at {first_path} and {duplicate_path}"
      ),
      Self::MissingEssentialContainer => f.write_str(
        "CargoSpec.Containers must contain at least one essential container",
      ),
      Self::NonEssentialInitContainer(name) => {
        write!(f, "CargoSpec.InitContainers[{name}].Essential must be true")
      }
    }
  }
}

impl std::error::Error for CargoSpecValidationError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::InvalidContainer { source, .. } => Some(source),
      _ => None,
    }
  }
}

fn container_path(collection: &str, index: usize, name: &str) -> String {
  if name.is_empty() {
    format!("{collection}[{index}]")
  } else {
    format!("{collection}[{name}]")
  }
}

fn validate_cargo_declaration(
  name: &str,
  replicas: usize,
  init_containers: &[ContainerSpec],
  containers: &[ContainerSpec],
) -> Result<(), CargoSpecValidationError> {
  if name.is_empty() {
    return Err(CargoSpecValidationError::EmptyName);
  }
  if replicas == 0 {
    return Err(CargoSpecValidationError::InvalidReplicaCount);
  }
  if containers.is_empty() {
    return Err(CargoSpecValidationError::EmptyContainers);
  }

  let mut names = HashMap::new();
  for (index, container) in containers.iter().enumerate() {
    let path = container_path("Containers", index, &container.name);
    container.validate().map_err(|source| {
      CargoSpecValidationError::InvalidContainer {
        path: path.clone(),
        source,
      }
    })?;
    if container.name == CARGO_SANDBOX_CONTAINER_NAME {
      return Err(CargoSpecValidationError::ReservedContainerName {
        path,
        name: container.name.clone(),
      });
    }
    if let Some(first_path) =
      names.insert(container.name.as_str(), path.clone())
    {
      return Err(CargoSpecValidationError::DuplicateContainerName {
        name: container.name.clone(),
        first_path,
        duplicate_path: path,
      });
    }
  }
  if !containers.iter().any(|container| container.essential) {
    return Err(CargoSpecValidationError::MissingEssentialContainer);
  }

  for (index, container) in init_containers.iter().enumerate() {
    let path = container_path("InitContainers", index, &container.name);
    container.validate().map_err(|source| {
      CargoSpecValidationError::InvalidContainer {
        path: path.clone(),
        source,
      }
    })?;
    if container.name == CARGO_SANDBOX_CONTAINER_NAME {
      return Err(CargoSpecValidationError::ReservedContainerName {
        path,
        name: container.name.clone(),
      });
    }
    if !container.essential {
      return Err(CargoSpecValidationError::NonEssentialInitContainer(
        container.name.clone(),
      ));
    }
    if let Some(first_path) =
      names.insert(container.name.as_str(), path.clone())
    {
      return Err(CargoSpecValidationError::DuplicateContainerName {
        name: container.name.clone(),
        first_path,
        duplicate_path: path,
      });
    }
  }

  Ok(())
}

impl CargoSpec {
  /// Validate the complete desired declaration without daemon or Docker state.
  pub fn validate(&self) -> Result<(), CargoSpecValidationError> {
    validate_cargo_declaration(
      &self.name,
      self.replicas,
      &self.init_containers,
      &self.containers,
    )
  }
}

/// Payload used to replace selected fields of a Cargo declaration.
///
/// Collection values replace the complete corresponding collection after the
/// patch is merged by the API layer.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoSpecPatch {
  /// New name of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  /// New metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// New replica count.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(minimum = 1))]
  #[cfg_attr(feature = "schemars", schemars(range(min = 1)))]
  pub replicas: Option<usize>,
  /// New Cargo-owned network mode.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub network_mode: Option<CargoNetworkMode>,
  /// Replacement Cargo-owned port bindings.
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      skip_serializing_if = "Option::is_none",
      deserialize_with = "strict_bollard::deserialize_optional_port_map"
    )
  )]
  pub port_bindings: Option<PortMap>,
  /// Replacement Cargo-level shared secrets.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  /// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
  /// Replacement ordered init-container declaration.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_containers: Option<Vec<ContainerSpec>>,
  /// Replacement application-container declaration.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub containers: Option<Vec<ContainerSpec>>,
}

/// Stored revision of a Cargo declaration, including immutable revision
/// metadata.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoSpecRevision {
  /// Unique identifier of the cargo spec
  pub key: uuid::Uuid,
  /// Canonical cargo key in `{namespace}.{name}` format
  pub cargo_key: String,
  /// Version of the spec
  pub version: String,
  /// Creation date of the cargo spec
  pub created_at: chrono::NaiveDateTime,
  /// Name of the cargo
  pub name: String,
  /// Metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Number of durable Cargo replicas.
  #[cfg_attr(feature = "serde", serde(default = "default_cargo_replicas"))]
  #[cfg_attr(
    feature = "utoipa",
    schema(default = default_cargo_replicas, minimum = 1)
  )]
  #[cfg_attr(feature = "schemars", schemars(range(min = 1)))]
  pub replicas: usize,
  /// Cargo-owned network shared by the revision's containers.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub network_mode: Option<CargoNetworkMode>,
  /// Cargo-owned host port bindings.
  #[cfg_attr(
    feature = "serde",
    serde(
      default,
      skip_serializing_if = "Option::is_none",
      deserialize_with = "strict_bollard::deserialize_optional_port_map"
    )
  )]
  pub port_bindings: Option<PortMap>,
  /// Secrets inherited by every regular and init container.
  #[cfg_attr(feature = "serde", serde(default))]
  pub secrets: Vec<String>,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  //// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
  /// Init containers executed in declaration order.
  #[cfg_attr(feature = "serde", serde(default))]
  pub init_containers: Vec<ContainerSpec>,
  /// Application containers in each Cargo replica.
  pub containers: Vec<ContainerSpec>,
}

impl Default for CargoSpecRevision {
  fn default() -> Self {
    Self {
      key: uuid::Uuid::nil(),
      cargo_key: String::new(),
      version: String::new(),
      created_at: Default::default(),
      name: String::new(),
      metadata: None,
      replicas: default_cargo_replicas(),
      network_mode: None,
      port_bindings: None,
      secrets: Vec::new(),
      placement: None,
      resource_requirement: None,
      init_containers: Vec::new(),
      containers: Vec::new(),
    }
  }
}

impl CargoSpecRevision {
  /// Validate the declaration stored by this revision.
  pub fn validate(&self) -> Result<(), CargoSpecValidationError> {
    validate_cargo_declaration(
      &self.name,
      self.replicas,
      &self.init_containers,
      &self.containers,
    )
  }
}

#[cfg(all(test, feature = "schemars"))]
mod tests {
  use super::*;

  #[test]
  fn nanocl_container_fields_do_not_collide_with_pinned_bollard_config() {
    let schema = serde_json::to_value(schemars::schema_for!(Config)).unwrap();
    let properties = schema["properties"]
      .as_object()
      .expect("Bollard Config schema must expose its properties");

    assert!(properties.contains_key("Image"));
    assert!(properties.contains_key("HostConfig"));
    for field in NANOCL_CONTAINER_FIELDS {
      assert!(
        !properties.contains_key(field),
        "Bollard Config field {field:?} collides with ContainerSpec's Nanocl namespace"
      );
    }
  }
}
