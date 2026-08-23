use std::{collections::HashMap, net::IpAddr};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "utoipa")]
use bollard_next::secret::GenericResourcesInner;

/// Generic namespace query filter
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericNspQuery {
  /// Name of the namespace
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub namespace: Option<String>,
}

impl GenericNspQuery {
  /// Create a new query with an optional namespace
  pub fn new(namespace: Option<&str>) -> Self {
    Self {
      namespace: namespace.map(|s| s.to_owned()),
    }
  }
}

/// Generic count response
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct GenericCount {
  /// Number of items
  pub count: i64,
}

/// Generic where clause
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum GenericClause {
  /// Equal
  Eq(String),
  /// Not equal
  Ne(String),
  /// Greater than
  Gt(String),
  /// Less than
  Lt(String),
  /// Greater than or equal
  Ge(String),
  /// Less than or equal
  Le(String),
  /// Like
  Like(String),
  /// Not like
  NotLike(String),
  /// In
  In(Vec<String>),
  /// Not in
  NotIn(Vec<String>),
  /// Is null
  IsNull,
  /// Is not null
  IsNotNull,
  /// TODO: Add Between
  // Between(String, String),
  /// JSON contains
  Contains(serde_json::Value),
  /// JSON Has key
  HasKey(String),
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericWhere {
  #[cfg_attr(feature = "serde", serde(flatten))]
  pub conditions: HashMap<String, GenericClause>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub or: Option<Vec<HashMap<String, GenericClause>>>,
}

/// Generic order enum
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GenericOrder {
  /// Ascending
  Asc,
  /// Descending
  Desc,
}

impl std::str::FromStr for GenericOrder {
  type Err = std::io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "asc" => Ok(Self::Asc),
      "desc" => Ok(Self::Desc),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "Invalid order",
      )),
    }
  }
}

/// Generic filter for list operation
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericFilter {
  /// Where clause
  #[cfg_attr(
    feature = "serde",
    serde(rename = "where", skip_serializing_if = "Option::is_none")
  )]
  pub r#where: Option<GenericWhere>,
  /// Limit number of items default (100)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub limit: Option<usize>,
  /// Offset to navigate through items
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub offset: Option<usize>,
  /// Order by
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub order_by: Option<Vec<String>>,
}

/// Generic query string parameters for list operations
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericListQuery {
  /// A json as string as GenericFilter
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub filter: Option<String>,
}

#[cfg(feature = "serde")]
impl TryFrom<GenericFilter> for GenericListQuery {
  type Error = serde_json::Error;

  fn try_from(filter: GenericFilter) -> Result<Self, Self::Error> {
    Ok(Self {
      filter: Some(serde_json::to_string(&filter)?),
    })
  }
}

#[cfg(feature = "serde")]
impl TryFrom<GenericListQuery> for GenericFilter {
  type Error = serde_json::Error;

  fn try_from(query: GenericListQuery) -> Result<Self, Self::Error> {
    match query.filter {
      None => Ok(Self::default()),
      Some(filter) => serde_json::from_str(&filter),
    }
  }
}

#[cfg(feature = "serde")]
impl TryFrom<GenericListQueryNsp> for GenericFilter {
  type Error = serde_json::Error;

  fn try_from(query: GenericListQueryNsp) -> Result<Self, Self::Error> {
    let filter = match query.filter {
      None => Self::default(),
      Some(filter) => serde_json::from_str(&filter)?,
    };
    Ok(filter)
  }
}

/// Generic query string parameters for list operations that include a namespace
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericListQueryNsp {
  /// A json as string as GenericFilter
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub filter: Option<String>,
  /// Name of the namespace
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub namespace: Option<String>,
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericFilterNsp {
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub filter: Option<GenericFilter>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub namespace: Option<String>,
}

impl GenericListQueryNsp {
  pub fn new(namespace: Option<&str>) -> Self {
    Self {
      namespace: namespace.map(|s| s.to_owned()),
      ..Default::default()
    }
  }

  pub fn with_namespace(mut self, namespace: Option<&str>) -> Self {
    self.namespace = namespace.map(|s| s.to_owned());
    self
  }
}

#[cfg(feature = "serde")]
impl TryFrom<GenericFilterNsp> for GenericListQueryNsp {
  type Error = serde_json::Error;

  fn try_from(filter: GenericFilterNsp) -> Result<Self, Self::Error> {
    let formatted_filter = match filter.filter {
      None => None,
      Some(filter) => Some(serde_json::to_string(&filter)?),
    };
    Ok(Self {
      filter: formatted_filter,
      namespace: filter.namespace,
    })
  }
}

#[cfg(feature = "serde")]
impl TryFrom<GenericListQueryNsp> for GenericFilterNsp {
  type Error = serde_json::Error;

  fn try_from(query: GenericListQueryNsp) -> Result<Self, Self::Error> {
    let filter = match query.filter {
      None => None,
      Some(filter) => Some(serde_json::from_str(&filter)?),
    };
    Ok(GenericFilterNsp {
      filter,
      namespace: query.namespace,
    })
  }
}

#[cfg(feature = "serde")]
impl TryFrom<GenericFilter> for GenericListQueryNsp {
  type Error = serde_json::Error;

  fn try_from(filter: GenericFilter) -> Result<Self, Self::Error> {
    Ok(Self {
      filter: Some(serde_json::to_string(&filter)?),
      ..Default::default()
    })
  }
}

impl GenericFilter {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn limit(mut self, limit: usize) -> Self {
    self.limit = Some(limit);
    self
  }

  pub fn offset(mut self, offset: usize) -> Self {
    self.offset = Some(offset);
    self
  }

  pub fn r#where(mut self, key: &str, clause: GenericClause) -> Self {
    if self.r#where.is_none() {
      self.r#where = Some(GenericWhere::default());
    }
    self
      .r#where
      .as_mut()
      .unwrap()
      .conditions
      .insert(key.to_owned(), clause);
    self
  }
}

/// Policy for pulling images related to process objects (job, cargo, vm)
#[derive(Default, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ImagePullPolicy {
  /// Never try to pull the image (image should be loaded manually then)
  Never,
  /// Always try to pull image on the node before starting the cargo/job
  Always,
  /// Pull the image only if it not exist on the node
  #[default]
  IfNotPresent,
}

/// Network binding kinds
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NetworkKind {
  /// All networks
  All,
  /// Only 127.0.0.1
  Local,
  /// Only public ip addresses
  Public,
  /// Only internal ip addresses
  Internal,
  /// A user-defined container network
  Named(String),
  /// Specific ip address
  Other(IpAddr),
}

#[cfg(feature = "serde")]
impl Serialize for NetworkKind {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    match self {
      Self::All => serializer.serialize_str("All"),
      Self::Local => serializer.serialize_str("Local"),
      Self::Public => serializer.serialize_str("Public"),
      Self::Internal => serializer.serialize_str("Internal"),
      // Named networks intentionally use the compact string representation so
      // state files can use `Network: private-api`. Names which would be
      // interpreted as a reserved selector or IP address keep the tagged
      // escape form so every variant remains round-trip safe.
      Self::Named(name) => {
        if name.is_empty() {
          return Err(serde::ser::Error::custom(
            "network name cannot be empty",
          ));
        }
        if matches!(name.as_str(), "All" | "Local" | "Public" | "Internal")
          || name.parse::<IpAddr>().is_ok()
        {
          use serde::ser::SerializeMap;
          let mut map = serializer.serialize_map(Some(1))?;
          map.serialize_entry("Named", name)?;
          map.end()
        } else {
          serializer.serialize_str(name)
        }
      }
      // Keep the externally-tagged representation emitted by the previous
      // derived serde implementation for backwards compatibility.
      Self::Other(ip) => {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("Other", ip)?;
        map.end()
      }
    }
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for NetworkKind {
  fn name() -> std::borrow::Cow<'static, str> {
    "NetworkKind".into()
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for NetworkKind {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::SchemaType;
    use utoipa::openapi::{ObjectBuilder, OneOfBuilder, Type};

    let string = ObjectBuilder::new()
      .schema_type(SchemaType::Type(Type::String))
      .description(Some(
        "A reserved selector, IP address, or user-defined network name",
      ));
    let tagged_named = ObjectBuilder::new()
      .schema_type(SchemaType::Type(Type::Object))
      .description(Some(
        "Tagged escape form for a network name which looks like a reserved selector or IP address",
      ))
      .property(
        "Named",
        ObjectBuilder::new()
          .schema_type(SchemaType::Type(Type::String))
          .build(),
      )
      .required("Named");
    let tagged_ip = ObjectBuilder::new()
      .schema_type(SchemaType::Type(Type::Object))
      .description(Some("Backwards-compatible tagged IP address form"))
      .property(
        "Other",
        ObjectBuilder::new()
          .schema_type(SchemaType::Type(Type::String))
          .build(),
      )
      .required("Other");

    utoipa::openapi::schema::Schema::OneOf(
      OneOfBuilder::new()
        .item(string)
        .item(tagged_named)
        .item(tagged_ip)
        .description(Some("Network binding selector"))
        .build(),
    )
    .into()
  }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for NetworkKind {
  fn schema_name() -> String {
    "NetworkKind".to_owned()
  }

  fn json_schema(
    _generator: &mut schemars::r#gen::SchemaGenerator,
  ) -> schemars::schema::Schema {
    serde_json::from_value(serde_json::json!({
      "description": "Network binding selector",
      "oneOf": [
        {
          "type": "string",
          "minLength": 1,
          "description": "A reserved selector, IP address, or user-defined network name"
        },
        {
          "type": "object",
          "properties": {
            "Named": { "type": "string", "minLength": 1 }
          },
          "required": ["Named"],
          "additionalProperties": false
        },
        {
          "type": "object",
          "properties": {
            "Other": { "type": "string" }
          },
          "required": ["Other"],
          "additionalProperties": false
        }
      ]
    }))
    .expect("the static NetworkKind schema must be valid")
  }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for NetworkKind {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    use serde::de::Error;

    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
      serde_json::Value::String(value) => match value.as_str() {
        "All" => Ok(Self::All),
        "Local" => Ok(Self::Local),
        "Public" => Ok(Self::Public),
        "Internal" => Ok(Self::Internal),
        "" => Err(D::Error::custom("network name cannot be empty")),
        value => match value.parse::<IpAddr>() {
          Ok(ip) => Ok(Self::Other(ip)),
          Err(_) => Ok(Self::Named(value.to_owned())),
        },
      },
      serde_json::Value::Object(mut value) if value.len() == 1 => {
        if let Some(value) = value.remove("Other") {
          let value = value.as_str().ok_or_else(|| {
            D::Error::custom("Other network must contain an IP address")
          })?;
          return value.parse::<IpAddr>().map(Self::Other).map_err(|err| {
            D::Error::custom(format!("invalid network IP address: {err}"))
          });
        }
        if let Some(value) = value.remove("Named") {
          let value = value.as_str().ok_or_else(|| {
            D::Error::custom("Named network must contain a name")
          })?;
          if value.is_empty() {
            return Err(D::Error::custom("network name cannot be empty"));
          }
          return Ok(Self::Named(value.to_owned()));
        }
        Err(D::Error::custom("unknown network selector"))
      }
      _ => Err(D::Error::custom(
        "network must be All, Local, Public, Internal, an IP address, or a network name",
      )),
    }
  }
}

impl std::fmt::Display for NetworkKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      NetworkKind::All => write!(f, "All"),
      NetworkKind::Local => write!(f, "Local"),
      NetworkKind::Public => write!(f, "Public"),
      NetworkKind::Internal => write!(f, "Internal"),
      NetworkKind::Named(name) => write!(f, "{name}"),
      NetworkKind::Other(ip) => write!(f, "Ip({})", ip),
    }
  }
}

#[cfg(feature = "utoipa")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(untagged))]
pub enum Primitive {
  String(String),
  Number(f64),
  Bool(bool),
}

/// Helper to generate have Any type for [OpenApi](OpenApi) useful for dynamic json objects like [ResourceSpec](ResourceSpec)
#[cfg(feature = "utoipa")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(untagged))]
pub enum Any {
  String(String),
  Number(f64),
  Bool(bool),
  Array(Vec<Primitive>),
  Object(HashMap<String, Primitive>),
}

#[cfg(feature = "utoipa")]
pub struct EmptyObject;

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for EmptyObject {
  fn name() -> std::borrow::Cow<'static, str> {
    "EmptyObject".into()
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for EmptyObject {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    utoipa::openapi::ObjectBuilder::new()
      .title(Some("EmptyObject"))
      .description(Some("EmptyObject"))
      .schema_type(utoipa::openapi::schema::SchemaType::Type(
        utoipa::openapi::Type::Object,
      ))
      .build()
      .into()
  }
}

#[cfg(feature = "utoipa")]
pub struct GenericResources;

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for GenericResources {
  fn name() -> std::borrow::Cow<'static, str> {
    "GenericResources".into()
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for GenericResources {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    GenericResourcesInner::schema()
  }
}

#[cfg(feature = "utoipa")]
pub struct BollardDate;

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for BollardDate {
  fn name() -> std::borrow::Cow<'static, str> {
    "BollardDate".into()
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for BollardDate {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    utoipa::openapi::ObjectBuilder::new()
      .title(Some("BollardDate"))
      .description(Some("BollardDate"))
      .schema_type(utoipa::openapi::schema::SchemaType::Type(
        utoipa::openapi::Type::String,
      ))
      .examples(["2021-01-01T00:00:00.000000000Z"])
      .build()
      .into()
  }
}

#[cfg(feature = "utoipa")]
pub struct PortMap;

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for PortMap {
  fn name() -> std::borrow::Cow<'static, str> {
    "PortMap".into()
  }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for PortMap {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    utoipa::openapi::ObjectBuilder::new()
      .title(Some("PortMap"))
      .description(Some("PortMap"))
      .schema_type(utoipa::openapi::schema::SchemaType::Type(
        utoipa::openapi::Type::Object,
      ))
      .property(
        "<port/tcp|udp>",
        utoipa::openapi::ArrayBuilder::new()
          .items(
            utoipa::openapi::ObjectBuilder::new()
              .property(
                "HostPort",
                utoipa::openapi::ObjectBuilder::new()
                  .schema_type(utoipa::openapi::schema::SchemaType::Type(
                    utoipa::openapi::Type::String,
                  ))
                  .build(),
              )
              .property(
                "HostIp",
                utoipa::openapi::ObjectBuilder::new()
                  .schema_type(utoipa::openapi::schema::SchemaType::Type(
                    utoipa::openapi::schema::Type::String,
                  ))
                  .build(),
              )
              .build(),
          )
          .build(),
      )
      .into()
  }
}

#[cfg(all(test, feature = "serde"))]
mod network_kind_tests {
  use super::NetworkKind;

  #[test]
  fn named_network_uses_compact_string_serde() {
    let network: NetworkKind =
      serde_json::from_str(r#""private-api""#).unwrap();
    assert_eq!(network, NetworkKind::Named("private-api".to_owned()));
    assert_eq!(serde_json::to_string(&network).unwrap(), r#""private-api""#);
  }

  #[test]
  fn reserved_network_strings_stay_backwards_compatible() {
    for (value, expected) in [
      ("All", NetworkKind::All),
      ("Local", NetworkKind::Local),
      ("Public", NetworkKind::Public),
      ("Internal", NetworkKind::Internal),
    ] {
      let json = serde_json::to_string(value).unwrap();
      let network: NetworkKind = serde_json::from_str(&json).unwrap();
      assert_eq!(network, expected);
      assert_eq!(serde_json::to_string(&network).unwrap(), json);
    }
  }

  #[test]
  fn ip_network_accepts_legacy_and_compact_forms() {
    let legacy = r#"{"Other":"10.20.30.40"}"#;
    let expected = NetworkKind::Other("10.20.30.40".parse().unwrap());
    assert_eq!(
      serde_json::from_str::<NetworkKind>(legacy).unwrap(),
      expected
    );
    assert_eq!(serde_json::to_string(&expected).unwrap(), legacy);
    assert_eq!(
      serde_json::from_str::<NetworkKind>(r#""10.20.30.40""#).unwrap(),
      expected
    );
  }

  #[test]
  fn named_tag_is_accepted_but_normalized() {
    let network: NetworkKind =
      serde_json::from_str(r#"{"Named":"private-api"}"#).unwrap();
    assert_eq!(network, NetworkKind::Named("private-api".to_owned()));
    assert_eq!(serde_json::to_string(&network).unwrap(), r#""private-api""#);
  }

  #[test]
  fn ambiguous_named_networks_keep_the_tagged_escape_form() {
    for name in ["Local", "10.20.30.40", "2001:db8::1"] {
      let expected = NetworkKind::Named(name.to_owned());
      let json = serde_json::to_string(&expected).unwrap();
      assert_eq!(json, format!(r#"{{"Named":"{name}"}}"#));
      assert_eq!(
        serde_json::from_str::<NetworkKind>(&json).unwrap(),
        expected
      );
    }
  }

  #[test]
  fn empty_named_network_cannot_be_serialized() {
    assert!(serde_json::to_string(&NetworkKind::Named(String::new())).is_err());
  }

  #[cfg(feature = "utoipa")]
  #[test]
  fn openapi_schema_matches_the_compact_and_tagged_forms() {
    use utoipa::PartialSchema;

    let schema = serde_json::to_value(NetworkKind::schema()).unwrap();
    assert_eq!(schema["oneOf"][0]["type"], "string");
    assert_eq!(schema["oneOf"][1]["required"][0], "Named");
    assert_eq!(schema["oneOf"][2]["required"][0], "Other");
  }
}
