use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Generic namespace query filter
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericNspQuery {
  /// Name of the namespace
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
  #[cfg_attr(feature = "serde", serde(rename = "where"))]
  pub r#where: Option<GenericWhere>,
  /// Limit number of items default (100)
  pub limit: Option<usize>,
  /// Offset to navigate through items
  pub offset: Option<usize>,
  /// Order by
  pub order_by: Option<Vec<String>>,
}

/// Generic query string parameters for list operations
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericListQuery {
  /// A json as string as GenericFilter
  pub filter: Option<String>,
}

impl TryFrom<GenericFilter> for GenericListQuery {
  type Error = serde_json::Error;

  fn try_from(filter: GenericFilter) -> Result<Self, Self::Error> {
    Ok(Self {
      filter: Some(serde_json::to_string(&filter)?),
    })
  }
}

impl TryFrom<GenericListQuery> for GenericFilter {
  type Error = serde_json::Error;

  fn try_from(query: GenericListQuery) -> Result<Self, Self::Error> {
    match query.filter {
      None => Ok(Self::default()),
      Some(filter) => serde_json::from_str(&filter),
    }
  }
}

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
  pub filter: Option<String>,
  pub namespace: Option<String>,
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericFilterNsp {
  pub filter: Option<GenericFilter>,
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
