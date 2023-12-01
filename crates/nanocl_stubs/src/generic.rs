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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct GenericCount {
  /// Number of items
  pub count: i64,
}

/// Generic where clause
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

/// Generic filter for list operation
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericFilter {
  /// Where clause
  #[cfg_attr(feature = "serde", serde(rename = "where"))]
  pub r#where: Option<HashMap<String, GenericClause>>,
}

/// Generic query string parameters for list operations
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

impl TryFrom<GenericListNspQuery> for GenericFilter {
  type Error = serde_json::Error;

  fn try_from(query: GenericListNspQuery) -> Result<Self, Self::Error> {
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericListNspQuery {
  /// A json as string as GenericFilter
  pub filter: Option<String>,
  pub namespace: Option<String>,
}

impl GenericFilter {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn r#where(mut self, key: &str, clause: GenericClause) -> Self {
    if self.r#where.is_none() {
      self.r#where = Some(HashMap::new());
    }
    self
      .r#where
      .as_mut()
      .unwrap()
      .insert(key.to_owned(), clause);
    self
  }
}
