mod runtime;
mod sql;

pub use runtime::{RuntimeError, with_connection, with_raw_query};
pub use sql::{RenderedFilter, SqlBackend};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[cfg(feature = "derive")]
pub use prismar_derive::PrismarModel;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum ScalarValue {
  Null,
  Bool(bool),
  Number(f64),
  String(String),
  Json(serde_json::Value),
  Uuid(uuid::Uuid),
  DateTime(NaiveDateTime),
}

impl From<String> for ScalarValue {
  fn from(value: String) -> Self {
    Self::String(value)
  }
}

impl From<&str> for ScalarValue {
  fn from(value: &str) -> Self {
    Self::String(value.to_owned())
  }
}

impl From<bool> for ScalarValue {
  fn from(value: bool) -> Self {
    Self::Bool(value)
  }
}

impl From<serde_json::Value> for ScalarValue {
  fn from(value: serde_json::Value) -> Self {
    Self::Json(value)
  }
}

impl From<uuid::Uuid> for ScalarValue {
  fn from(value: uuid::Uuid) -> Self {
    Self::Uuid(value)
  }
}

impl From<NaiveDateTime> for ScalarValue {
  fn from(value: NaiveDateTime) -> Self {
    Self::DateTime(value)
  }
}

macro_rules! impl_number_value {
  ($($ty:ty),* $(,)?) => {
    $(
      impl From<$ty> for ScalarValue {
        fn from(value: $ty) -> Self {
          Self::Number(value as f64)
        }
      }
    )*
  };
}

impl_number_value!(
  i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum StringFilter {
  Equals(String),
  NotEquals(String),
  Like(String),
  NotLike(String),
  Contains(String),
  NotContains(String),
  StartsWith(String),
  EndsWith(String),
  GreaterThan(String),
  GreaterThanOrEquals(String),
  LessThan(String),
  LessThanOrEquals(String),
  In(Vec<String>),
  NotIn(Vec<String>),
  IsNull,
  IsNotNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum NumberFilter {
  Equals(f64),
  NotEquals(f64),
  GreaterThan(f64),
  GreaterThanOrEquals(f64),
  LessThan(f64),
  LessThanOrEquals(f64),
  In(Vec<f64>),
  NotIn(Vec<f64>),
  IsNull,
  IsNotNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum BoolFilter {
  Equals(bool),
  NotEquals(bool),
  In(Vec<bool>),
  NotIn(Vec<bool>),
  IsNull,
  IsNotNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum UuidFilter {
  Equals(uuid::Uuid),
  NotEquals(uuid::Uuid),
  In(Vec<uuid::Uuid>),
  NotIn(Vec<uuid::Uuid>),
  IsNull,
  IsNotNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum DateTimeFilter {
  Equals(NaiveDateTime),
  NotEquals(NaiveDateTime),
  GreaterThan(NaiveDateTime),
  GreaterThanOrEquals(NaiveDateTime),
  LessThan(NaiveDateTime),
  LessThanOrEquals(NaiveDateTime),
  In(Vec<NaiveDateTime>),
  NotIn(Vec<NaiveDateTime>),
  IsNull,
  IsNotNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum JsonFilter {
  Equals(serde_json::Value),
  NotEquals(serde_json::Value),
  Contains(serde_json::Value),
  NotContains(serde_json::Value),
  HasKey(String),
  HasAnyKey(Vec<String>),
  HasEveryKey(Vec<String>),
  PathEquals {
    path: Vec<String>,
    value: serde_json::Value,
  },
  PathNotEquals {
    path: Vec<String>,
    value: serde_json::Value,
  },
  PathLike {
    path: Vec<String>,
    value: String,
  },
  PathNotLike {
    path: Vec<String>,
    value: String,
  },
  PathStartsWith {
    path: Vec<String>,
    value: String,
  },
  PathEndsWith {
    path: Vec<String>,
    value: String,
  },
  PathContains {
    path: Vec<String>,
    value: serde_json::Value,
  },
  IsNull,
  IsNotNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum FieldFilter {
  String(StringFilter),
  Number(NumberFilter),
  Bool(BoolFilter),
  Uuid(UuidFilter),
  DateTime(DateTimeFilter),
  Json(JsonFilter),
  Null,
  NotNull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum RelationFilterOp {
  Some,
  Every,
  None,
  Is,
  IsNot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Predicate {
  pub field: String,
  pub filter: FieldFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct RelationPredicate {
  pub field: String,
  pub op: RelationFilterOp,
  pub filter: Box<ModelFilter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum Condition {
  Predicate(Predicate),
  And(Vec<ModelFilter>),
  Or(Vec<ModelFilter>),
  Not(Box<ModelFilter>),
  Relation(RelationPredicate),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ModelFilter {
  pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum OrderDirection {
  Asc,
  Desc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct OrderBy {
  pub field: String,
  pub direction: OrderDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
  pub skip: Option<usize>,
  pub take: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ReadManyArgs {
  #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
  pub r#where: Option<ModelFilter>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub order_by: Option<Vec<OrderBy>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ReadUniqueArgs {
  #[serde(rename = "where")]
  pub r#where: ModelFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CountArgs {
  #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
  pub r#where: Option<ModelFilter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CreateArgs {
  pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CreateManyArgs {
  pub data: Vec<serde_json::Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub skip_duplicates: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct UpdateArgs {
  #[serde(rename = "where")]
  pub r#where: ModelFilter,
  pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct UpdateManyArgs {
  #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
  pub r#where: Option<ModelFilter>,
  pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeleteArgs {
  #[serde(rename = "where")]
  pub r#where: ModelFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeleteManyArgs {
  #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
  pub r#where: Option<ModelFilter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct IdSelector {
  pub field: String,
  pub value: ScalarValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ReadByIdArgs {
  pub id: IdSelector,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeleteByIdArgs {
  pub id: IdSelector,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct RawSqlQuery {
  pub sql: String,
  pub params: Vec<ScalarValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawSqlBuilder {
  backend: SqlBackend,
  base: String,
  where_filter: Option<ModelFilter>,
  order_by: Vec<OrderBy>,
  limit: Option<usize>,
  offset: Option<usize>,
}

impl ModelFilter {
  pub fn empty() -> Self {
    Self::default()
  }

  pub fn predicate(
    mut self,
    field: impl Into<String>,
    filter: FieldFilter,
  ) -> Self {
    self.conditions.push(Condition::Predicate(Predicate {
      field: field.into(),
      filter,
    }));
    self
  }

  pub fn and_group(mut self, filters: Vec<ModelFilter>) -> Self {
    if !filters.is_empty() {
      self.conditions.push(Condition::And(filters));
    }
    self
  }

  pub fn or_group(mut self, filters: Vec<ModelFilter>) -> Self {
    if !filters.is_empty() {
      self.conditions.push(Condition::Or(filters));
    }
    self
  }

  pub fn not_group(mut self, filter: ModelFilter) -> Self {
    self.conditions.push(Condition::Not(Box::new(filter)));
    self
  }

  pub fn relation(
    mut self,
    field: impl Into<String>,
    op: RelationFilterOp,
    filter: ModelFilter,
  ) -> Self {
    self.conditions.push(Condition::Relation(RelationPredicate {
      field: field.into(),
      op,
      filter: Box::new(filter),
    }));
    self
  }

  pub fn render(&self, backend: SqlBackend) -> RenderedFilter {
    sql::render_model_filter(self, backend)
  }

  pub fn is_empty(&self) -> bool {
    self.conditions.is_empty()
  }

  pub fn read_by_id(
    field: impl Into<String>,
    value: impl IntoIdFilter,
  ) -> Self {
    Self::empty().predicate(field, value.into_field_filter())
  }

  pub fn delete_by_id(
    field: impl Into<String>,
    value: impl IntoIdFilter,
  ) -> DeleteArgs {
    DeleteArgs {
      r#where: Self::read_by_id(field, value),
    }
  }
}

impl IdSelector {
  pub fn new(field: impl Into<String>, value: impl Into<ScalarValue>) -> Self {
    Self {
      field: field.into(),
      value: value.into(),
    }
  }

  pub fn to_filter(&self) -> Result<ModelFilter, String> {
    if !is_safe_identifier(self.field.as_str()) {
      return Err("Invalid id selector field".to_owned());
    }
    Ok(ModelFilter::empty().predicate(
      self.field.clone(),
      scalar_to_field_filter(self.value.clone()),
    ))
  }
}

impl ReadByIdArgs {
  pub fn to_filter(&self) -> Result<ModelFilter, String> {
    self.id.to_filter()
  }
}

impl DeleteByIdArgs {
  pub fn to_filter(&self) -> Result<ModelFilter, String> {
    self.id.to_filter()
  }
}

impl RawSqlBuilder {
  pub fn new(
    backend: SqlBackend,
    base: impl Into<String>,
  ) -> Result<Self, String> {
    let base = base.into();
    if base.contains(';') {
      return Err("Raw base query must not contain ';'".to_owned());
    }
    Ok(Self {
      backend,
      base,
      where_filter: None,
      order_by: Vec::new(),
      limit: None,
      offset: None,
    })
  }

  pub fn filter(mut self, filter: ModelFilter) -> Self {
    self.where_filter = Some(filter);
    self
  }

  pub fn order_by(
    mut self,
    field: impl Into<String>,
    direction: OrderDirection,
  ) -> Result<Self, String> {
    let field = field.into();
    if !is_safe_identifier(&field) {
      return Err("Invalid order_by field".to_owned());
    }
    self.order_by.push(OrderBy { field, direction });
    Ok(self)
  }

  pub fn limit(mut self, limit: usize) -> Self {
    self.limit = Some(limit);
    self
  }

  pub fn offset(mut self, offset: usize) -> Self {
    self.offset = Some(offset);
    self
  }

  pub fn build(self) -> Result<RawSqlQuery, String> {
    let mut sql = self.base;
    let mut params = Vec::new();
    let mut index = 0usize;

    if let Some(filter) = self.where_filter {
      let rendered = filter.render(self.backend);
      if rendered.sql != "1=1" {
        sql.push_str(" WHERE ");
        sql.push_str(rendered.sql.as_str());
        params.extend(rendered.params);
        index = params.len();
      }
    }

    if !self.order_by.is_empty() {
      sql.push_str(" ORDER BY ");
      let parts = self
        .order_by
        .into_iter()
        .map(|item| {
          let direction = match item.direction {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
          };
          format!("{} {}", item.field, direction)
        })
        .collect::<Vec<_>>();
      sql.push_str(parts.join(", ").as_str());
    }

    if let Some(limit) = self.limit {
      index += 1;
      match self.backend {
        SqlBackend::Postgres => {
          sql.push_str(format!(" LIMIT ${index}").as_str())
        }
        SqlBackend::MySql | SqlBackend::Sqlite => sql.push_str(" LIMIT ?"),
      }
      params.push(ScalarValue::Number(limit as f64));
    }

    if let Some(offset) = self.offset {
      index += 1;
      match self.backend {
        SqlBackend::Postgres => {
          sql.push_str(format!(" OFFSET ${index}").as_str())
        }
        SqlBackend::MySql | SqlBackend::Sqlite => sql.push_str(" OFFSET ?"),
      }
      params.push(ScalarValue::Number(offset as f64));
    }

    Ok(RawSqlQuery { sql, params })
  }
}

pub trait IntoIdFilter {
  fn into_field_filter(self) -> FieldFilter;
}

impl IntoIdFilter for String {
  fn into_field_filter(self) -> FieldFilter {
    FieldFilter::String(StringFilter::Equals(self))
  }
}

impl IntoIdFilter for &str {
  fn into_field_filter(self) -> FieldFilter {
    FieldFilter::String(StringFilter::Equals(self.to_owned()))
  }
}

impl IntoIdFilter for uuid::Uuid {
  fn into_field_filter(self) -> FieldFilter {
    FieldFilter::Uuid(UuidFilter::Equals(self))
  }
}

impl IntoIdFilter for bool {
  fn into_field_filter(self) -> FieldFilter {
    FieldFilter::Bool(BoolFilter::Equals(self))
  }
}

impl IntoIdFilter for NaiveDateTime {
  fn into_field_filter(self) -> FieldFilter {
    FieldFilter::DateTime(DateTimeFilter::Equals(self))
  }
}

macro_rules! impl_number_id_filter {
  ($($ty:ty),* $(,)?) => {
    $(
      impl IntoIdFilter for $ty {
        fn into_field_filter(self) -> FieldFilter {
          FieldFilter::Number(NumberFilter::Equals(self as f64))
        }
      }
    )*
  };
}

impl_number_id_filter!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

fn scalar_to_field_filter(value: ScalarValue) -> FieldFilter {
  match value {
    ScalarValue::Null => FieldFilter::Null,
    ScalarValue::Bool(v) => FieldFilter::Bool(BoolFilter::Equals(v)),
    ScalarValue::Number(v) => FieldFilter::Number(NumberFilter::Equals(v)),
    ScalarValue::String(v) => FieldFilter::String(StringFilter::Equals(v)),
    ScalarValue::Json(v) => FieldFilter::Json(JsonFilter::Equals(v)),
    ScalarValue::Uuid(v) => FieldFilter::Uuid(UuidFilter::Equals(v)),
    ScalarValue::DateTime(v) => {
      FieldFilter::DateTime(DateTimeFilter::Equals(v))
    }
  }
}

fn is_safe_identifier(identifier: &str) -> bool {
  let mut chars = identifier.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  if !(first == '_' || first.is_ascii_alphabetic()) {
    return false;
  }
  chars.all(|ch| ch == '_' || ch == '.' || ch.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StringOps {
  filters: Vec<StringFilter>,
}

pub trait IntoStringOpsInput {
  fn into_ops(self) -> StringOps;
}

impl StringOps {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn eq(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::Equals(value.into()));
    self
  }

  pub fn ne(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::NotEquals(value.into()));
    self
  }

  pub fn like(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::Like(value.into()));
    self
  }

  pub fn not_like(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::NotLike(value.into()));
    self
  }

  pub fn contains(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::Contains(value.into()));
    self
  }

  pub fn not_contains(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::NotContains(value.into()));
    self
  }

  pub fn starts_with(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::StartsWith(value.into()));
    self
  }

  pub fn ends_with(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::EndsWith(value.into()));
    self
  }

  pub fn in_(mut self, value: Vec<String>) -> Self {
    self.filters.push(StringFilter::In(value));
    self
  }

  pub fn not_in(mut self, value: Vec<String>) -> Self {
    self.filters.push(StringFilter::NotIn(value));
    self
  }

  pub fn gt(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::GreaterThan(value.into()));
    self
  }

  pub fn gte(mut self, value: impl Into<String>) -> Self {
    self
      .filters
      .push(StringFilter::GreaterThanOrEquals(value.into()));
    self
  }

  pub fn lt(mut self, value: impl Into<String>) -> Self {
    self.filters.push(StringFilter::LessThan(value.into()));
    self
  }

  pub fn lte(mut self, value: impl Into<String>) -> Self {
    self
      .filters
      .push(StringFilter::LessThanOrEquals(value.into()));
    self
  }

  pub fn is_null(mut self) -> Self {
    self.filters.push(StringFilter::IsNull);
    self
  }

  pub fn is_not_null(mut self) -> Self {
    self.filters.push(StringFilter::IsNotNull);
    self
  }

  pub fn into_filters(self) -> Vec<StringFilter> {
    self.filters
  }
}

impl From<StringFilter> for StringOps {
  fn from(value: StringFilter) -> Self {
    Self {
      filters: vec![value],
    }
  }
}

impl IntoStringOpsInput for StringOps {
  fn into_ops(self) -> StringOps {
    self
  }
}

impl IntoStringOpsInput for StringFilter {
  fn into_ops(self) -> StringOps {
    self.into()
  }
}

impl<F> IntoStringOpsInput for F
where
  F: FnOnce(StringOps) -> StringOps,
{
  fn into_ops(self) -> StringOps {
    self(StringOps::new())
  }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NumberOps {
  filters: Vec<NumberFilter>,
}

pub trait IntoNumberOpsInput {
  fn into_ops(self) -> NumberOps;
}

impl NumberOps {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn eq(mut self, value: f64) -> Self {
    self.filters.push(NumberFilter::Equals(value));
    self
  }

  pub fn ne(mut self, value: f64) -> Self {
    self.filters.push(NumberFilter::NotEquals(value));
    self
  }

  pub fn gt(mut self, value: f64) -> Self {
    self.filters.push(NumberFilter::GreaterThan(value));
    self
  }

  pub fn gte(mut self, value: f64) -> Self {
    self.filters.push(NumberFilter::GreaterThanOrEquals(value));
    self
  }

  pub fn lt(mut self, value: f64) -> Self {
    self.filters.push(NumberFilter::LessThan(value));
    self
  }

  pub fn lte(mut self, value: f64) -> Self {
    self.filters.push(NumberFilter::LessThanOrEquals(value));
    self
  }

  pub fn in_(mut self, value: Vec<f64>) -> Self {
    self.filters.push(NumberFilter::In(value));
    self
  }

  pub fn not_in(mut self, value: Vec<f64>) -> Self {
    self.filters.push(NumberFilter::NotIn(value));
    self
  }

  pub fn is_null(mut self) -> Self {
    self.filters.push(NumberFilter::IsNull);
    self
  }

  pub fn is_not_null(mut self) -> Self {
    self.filters.push(NumberFilter::IsNotNull);
    self
  }

  pub fn into_filters(self) -> Vec<NumberFilter> {
    self.filters
  }
}

impl From<NumberFilter> for NumberOps {
  fn from(value: NumberFilter) -> Self {
    Self {
      filters: vec![value],
    }
  }
}

impl IntoNumberOpsInput for NumberOps {
  fn into_ops(self) -> NumberOps {
    self
  }
}

impl IntoNumberOpsInput for NumberFilter {
  fn into_ops(self) -> NumberOps {
    self.into()
  }
}

impl<F> IntoNumberOpsInput for F
where
  F: FnOnce(NumberOps) -> NumberOps,
{
  fn into_ops(self) -> NumberOps {
    self(NumberOps::new())
  }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoolOps {
  filters: Vec<BoolFilter>,
}

pub trait IntoBoolOpsInput {
  fn into_ops(self) -> BoolOps;
}

impl BoolOps {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn eq(mut self, value: bool) -> Self {
    self.filters.push(BoolFilter::Equals(value));
    self
  }

  pub fn ne(mut self, value: bool) -> Self {
    self.filters.push(BoolFilter::NotEquals(value));
    self
  }

  pub fn in_(mut self, value: Vec<bool>) -> Self {
    self.filters.push(BoolFilter::In(value));
    self
  }

  pub fn not_in(mut self, value: Vec<bool>) -> Self {
    self.filters.push(BoolFilter::NotIn(value));
    self
  }

  pub fn is_null(mut self) -> Self {
    self.filters.push(BoolFilter::IsNull);
    self
  }

  pub fn is_not_null(mut self) -> Self {
    self.filters.push(BoolFilter::IsNotNull);
    self
  }

  pub fn into_filters(self) -> Vec<BoolFilter> {
    self.filters
  }
}

impl From<BoolFilter> for BoolOps {
  fn from(value: BoolFilter) -> Self {
    Self {
      filters: vec![value],
    }
  }
}

impl IntoBoolOpsInput for BoolOps {
  fn into_ops(self) -> BoolOps {
    self
  }
}

impl IntoBoolOpsInput for BoolFilter {
  fn into_ops(self) -> BoolOps {
    self.into()
  }
}

impl<F> IntoBoolOpsInput for F
where
  F: FnOnce(BoolOps) -> BoolOps,
{
  fn into_ops(self) -> BoolOps {
    self(BoolOps::new())
  }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UuidOps {
  filters: Vec<UuidFilter>,
}

pub trait IntoUuidOpsInput {
  fn into_ops(self) -> UuidOps;
}

impl UuidOps {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn eq(mut self, value: uuid::Uuid) -> Self {
    self.filters.push(UuidFilter::Equals(value));
    self
  }

  pub fn ne(mut self, value: uuid::Uuid) -> Self {
    self.filters.push(UuidFilter::NotEquals(value));
    self
  }

  pub fn in_(mut self, value: Vec<uuid::Uuid>) -> Self {
    self.filters.push(UuidFilter::In(value));
    self
  }

  pub fn not_in(mut self, value: Vec<uuid::Uuid>) -> Self {
    self.filters.push(UuidFilter::NotIn(value));
    self
  }

  pub fn is_null(mut self) -> Self {
    self.filters.push(UuidFilter::IsNull);
    self
  }

  pub fn is_not_null(mut self) -> Self {
    self.filters.push(UuidFilter::IsNotNull);
    self
  }

  pub fn into_filters(self) -> Vec<UuidFilter> {
    self.filters
  }
}

impl From<UuidFilter> for UuidOps {
  fn from(value: UuidFilter) -> Self {
    Self {
      filters: vec![value],
    }
  }
}

impl IntoUuidOpsInput for UuidOps {
  fn into_ops(self) -> UuidOps {
    self
  }
}

impl IntoUuidOpsInput for UuidFilter {
  fn into_ops(self) -> UuidOps {
    self.into()
  }
}

impl<F> IntoUuidOpsInput for F
where
  F: FnOnce(UuidOps) -> UuidOps,
{
  fn into_ops(self) -> UuidOps {
    self(UuidOps::new())
  }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DateTimeOps {
  filters: Vec<DateTimeFilter>,
}

pub trait IntoDateTimeOpsInput {
  fn into_ops(self) -> DateTimeOps;
}

impl DateTimeOps {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn eq(mut self, value: NaiveDateTime) -> Self {
    self.filters.push(DateTimeFilter::Equals(value));
    self
  }

  pub fn ne(mut self, value: NaiveDateTime) -> Self {
    self.filters.push(DateTimeFilter::NotEquals(value));
    self
  }

  pub fn gt(mut self, value: NaiveDateTime) -> Self {
    self.filters.push(DateTimeFilter::GreaterThan(value));
    self
  }

  pub fn gte(mut self, value: NaiveDateTime) -> Self {
    self
      .filters
      .push(DateTimeFilter::GreaterThanOrEquals(value));
    self
  }

  pub fn lt(mut self, value: NaiveDateTime) -> Self {
    self.filters.push(DateTimeFilter::LessThan(value));
    self
  }

  pub fn lte(mut self, value: NaiveDateTime) -> Self {
    self.filters.push(DateTimeFilter::LessThanOrEquals(value));
    self
  }

  pub fn in_(mut self, value: Vec<NaiveDateTime>) -> Self {
    self.filters.push(DateTimeFilter::In(value));
    self
  }

  pub fn not_in(mut self, value: Vec<NaiveDateTime>) -> Self {
    self.filters.push(DateTimeFilter::NotIn(value));
    self
  }

  pub fn is_null(mut self) -> Self {
    self.filters.push(DateTimeFilter::IsNull);
    self
  }

  pub fn is_not_null(mut self) -> Self {
    self.filters.push(DateTimeFilter::IsNotNull);
    self
  }

  pub fn into_filters(self) -> Vec<DateTimeFilter> {
    self.filters
  }
}

impl From<DateTimeFilter> for DateTimeOps {
  fn from(value: DateTimeFilter) -> Self {
    Self {
      filters: vec![value],
    }
  }
}

impl IntoDateTimeOpsInput for DateTimeOps {
  fn into_ops(self) -> DateTimeOps {
    self
  }
}

impl IntoDateTimeOpsInput for DateTimeFilter {
  fn into_ops(self) -> DateTimeOps {
    self.into()
  }
}

impl<F> IntoDateTimeOpsInput for F
where
  F: FnOnce(DateTimeOps) -> DateTimeOps,
{
  fn into_ops(self) -> DateTimeOps {
    self(DateTimeOps::new())
  }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct JsonOps {
  filters: Vec<JsonFilter>,
}

pub trait IntoJsonOpsInput {
  fn into_ops(self) -> JsonOps;
}

impl JsonOps {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn equals(mut self, value: serde_json::Value) -> Self {
    self.filters.push(JsonFilter::Equals(value));
    self
  }

  pub fn not_equals(mut self, value: serde_json::Value) -> Self {
    self.filters.push(JsonFilter::NotEquals(value));
    self
  }

  pub fn contains(mut self, value: serde_json::Value) -> Self {
    self.filters.push(JsonFilter::Contains(value));
    self
  }

  pub fn not_contains(mut self, value: serde_json::Value) -> Self {
    self.filters.push(JsonFilter::NotContains(value));
    self
  }

  pub fn has_key(mut self, key: impl Into<String>) -> Self {
    self.filters.push(JsonFilter::HasKey(key.into()));
    self
  }

  pub fn path_like(
    mut self,
    path: Vec<String>,
    value: impl Into<String>,
  ) -> Self {
    self.filters.push(JsonFilter::PathLike {
      path,
      value: value.into(),
    });
    self
  }

  pub fn path_not_like(
    mut self,
    path: Vec<String>,
    value: impl Into<String>,
  ) -> Self {
    self.filters.push(JsonFilter::PathNotLike {
      path,
      value: value.into(),
    });
    self
  }

  pub fn is_null(mut self) -> Self {
    self.filters.push(JsonFilter::IsNull);
    self
  }

  pub fn is_not_null(mut self) -> Self {
    self.filters.push(JsonFilter::IsNotNull);
    self
  }

  pub fn into_filters(self) -> Vec<JsonFilter> {
    self.filters
  }
}

impl From<JsonFilter> for JsonOps {
  fn from(value: JsonFilter) -> Self {
    Self {
      filters: vec![value],
    }
  }
}

impl IntoJsonOpsInput for JsonOps {
  fn into_ops(self) -> JsonOps {
    self
  }
}

impl IntoJsonOpsInput for JsonFilter {
  fn into_ops(self) -> JsonOps {
    self.into()
  }
}

impl<F> IntoJsonOpsInput for F
where
  F: FnOnce(JsonOps) -> JsonOps,
{
  fn into_ops(self) -> JsonOps {
    self(JsonOps::new())
  }
}

pub trait TypedFilter {
  fn into_model_filter(self) -> ModelFilter;
}

impl TypedFilter for ModelFilter {
  fn into_model_filter(self) -> ModelFilter {
    self
  }
}

#[cfg(feature = "nanocl_compat")]
mod nanocl_compat {
  use std::collections::HashMap;

  use nanocl_stubs::generic::{GenericClause, GenericFilter, GenericWhere};

  use crate::{
    BoolFilter, Condition, DateTimeFilter, FieldFilter, ModelFilter,
    NumberFilter, StringFilter, UuidFilter,
  };

  impl TryFrom<ModelFilter> for GenericFilter {
    type Error = String;

    fn try_from(value: ModelFilter) -> Result<Self, Self::Error> {
      let mut conditions = HashMap::new();
      for cond in value.conditions {
        let Condition::Predicate(pred) = cond else {
          continue;
        };
        let clause = map_clause(pred.filter)?;
        conditions.insert(pred.field, clause);
      }

      Ok(GenericFilter {
        r#where: Some(GenericWhere {
          conditions,
          or: None,
        }),
        ..Default::default()
      })
    }
  }

  fn map_clause(filter: FieldFilter) -> Result<GenericClause, String> {
    let clause = match filter {
      FieldFilter::String(StringFilter::Equals(v)) => GenericClause::Eq(v),
      FieldFilter::String(StringFilter::NotEquals(v)) => GenericClause::Ne(v),
      FieldFilter::String(StringFilter::Like(v)) => GenericClause::Like(v),
      FieldFilter::String(StringFilter::NotLike(v)) => {
        GenericClause::NotLike(v)
      }
      FieldFilter::String(StringFilter::Contains(v)) => {
        GenericClause::Like(format!("%{v}%"))
      }
      FieldFilter::String(StringFilter::NotContains(v)) => {
        GenericClause::NotLike(format!("%{v}%"))
      }
      FieldFilter::String(StringFilter::StartsWith(v)) => {
        GenericClause::Like(format!("{v}%"))
      }
      FieldFilter::String(StringFilter::EndsWith(v)) => {
        GenericClause::Like(format!("%{v}"))
      }
      FieldFilter::String(StringFilter::GreaterThan(v)) => GenericClause::Gt(v),
      FieldFilter::String(StringFilter::GreaterThanOrEquals(v)) => {
        GenericClause::Ge(v)
      }
      FieldFilter::String(StringFilter::LessThan(v)) => GenericClause::Lt(v),
      FieldFilter::String(StringFilter::LessThanOrEquals(v)) => {
        GenericClause::Le(v)
      }
      FieldFilter::String(StringFilter::In(v)) => GenericClause::In(v),
      FieldFilter::String(StringFilter::NotIn(v)) => GenericClause::NotIn(v),
      FieldFilter::String(StringFilter::IsNull) => GenericClause::IsNull,
      FieldFilter::String(StringFilter::IsNotNull) => GenericClause::IsNotNull,
      FieldFilter::Number(NumberFilter::Equals(v)) => {
        GenericClause::Eq(v.to_string())
      }
      FieldFilter::Number(NumberFilter::NotEquals(v)) => {
        GenericClause::Ne(v.to_string())
      }
      FieldFilter::Number(NumberFilter::GreaterThan(v)) => {
        GenericClause::Gt(v.to_string())
      }
      FieldFilter::Number(NumberFilter::GreaterThanOrEquals(v)) => {
        GenericClause::Ge(v.to_string())
      }
      FieldFilter::Number(NumberFilter::LessThan(v)) => {
        GenericClause::Lt(v.to_string())
      }
      FieldFilter::Number(NumberFilter::LessThanOrEquals(v)) => {
        GenericClause::Le(v.to_string())
      }
      FieldFilter::Number(NumberFilter::In(v)) => {
        GenericClause::In(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::Number(NumberFilter::NotIn(v)) => {
        GenericClause::NotIn(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::Number(NumberFilter::IsNull) => GenericClause::IsNull,
      FieldFilter::Number(NumberFilter::IsNotNull) => GenericClause::IsNotNull,
      FieldFilter::Bool(BoolFilter::Equals(v)) => {
        GenericClause::Eq(v.to_string())
      }
      FieldFilter::Bool(BoolFilter::NotEquals(v)) => {
        GenericClause::Ne(v.to_string())
      }
      FieldFilter::Bool(BoolFilter::In(v)) => {
        GenericClause::In(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::Bool(BoolFilter::NotIn(v)) => {
        GenericClause::NotIn(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::Bool(BoolFilter::IsNull) => GenericClause::IsNull,
      FieldFilter::Bool(BoolFilter::IsNotNull) => GenericClause::IsNotNull,
      FieldFilter::Uuid(UuidFilter::Equals(v)) => {
        GenericClause::Eq(v.to_string())
      }
      FieldFilter::Uuid(UuidFilter::NotEquals(v)) => {
        GenericClause::Ne(v.to_string())
      }
      FieldFilter::Uuid(UuidFilter::In(v)) => {
        GenericClause::In(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::Uuid(UuidFilter::NotIn(v)) => {
        GenericClause::NotIn(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::Uuid(UuidFilter::IsNull) => GenericClause::IsNull,
      FieldFilter::Uuid(UuidFilter::IsNotNull) => GenericClause::IsNotNull,
      FieldFilter::DateTime(DateTimeFilter::Equals(v)) => {
        GenericClause::Eq(v.to_string())
      }
      FieldFilter::DateTime(DateTimeFilter::NotEquals(v)) => {
        GenericClause::Ne(v.to_string())
      }
      FieldFilter::DateTime(DateTimeFilter::GreaterThan(v)) => {
        GenericClause::Gt(v.to_string())
      }
      FieldFilter::DateTime(DateTimeFilter::GreaterThanOrEquals(v)) => {
        GenericClause::Ge(v.to_string())
      }
      FieldFilter::DateTime(DateTimeFilter::LessThan(v)) => {
        GenericClause::Lt(v.to_string())
      }
      FieldFilter::DateTime(DateTimeFilter::LessThanOrEquals(v)) => {
        GenericClause::Le(v.to_string())
      }
      FieldFilter::DateTime(DateTimeFilter::In(v)) => {
        GenericClause::In(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::DateTime(DateTimeFilter::NotIn(v)) => {
        GenericClause::NotIn(v.into_iter().map(|it| it.to_string()).collect())
      }
      FieldFilter::DateTime(DateTimeFilter::IsNull) => GenericClause::IsNull,
      FieldFilter::DateTime(DateTimeFilter::IsNotNull) => {
        GenericClause::IsNotNull
      }
      FieldFilter::Json(crate::JsonFilter::Contains(v)) => {
        GenericClause::Contains(v)
      }
      FieldFilter::Json(crate::JsonFilter::HasKey(v)) => {
        GenericClause::HasKey(v)
      }
      FieldFilter::Json(crate::JsonFilter::IsNull) => GenericClause::IsNull,
      FieldFilter::Json(crate::JsonFilter::IsNotNull) => {
        GenericClause::IsNotNull
      }
      FieldFilter::Null => GenericClause::IsNull,
      FieldFilter::NotNull => GenericClause::IsNotNull,
      unsupported => {
        return Err(format!(
          "Unsupported GenericFilter conversion for filter: {unsupported:?}"
        ));
      }
    };
    Ok(clause)
  }
}
