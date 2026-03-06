use crate::{
  BoolFilter, Condition, DateTimeFilter, FieldFilter, JsonFilter, ModelFilter,
  NumberFilter, RelationFilterOp, ScalarValue, StringFilter, UuidFilter,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlBackend {
  Postgres,
  MySql,
  Sqlite,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedFilter {
  pub sql: String,
  pub params: Vec<ScalarValue>,
}

pub fn render_model_filter(
  filter: &ModelFilter,
  backend: SqlBackend,
) -> RenderedFilter {
  let mut params = Vec::new();
  let mut index = 0usize;
  let sql =
    render_conditions(&filter.conditions, backend, &mut params, &mut index);
  RenderedFilter { sql, params }
}

fn render_conditions(
  conditions: &[Condition],
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  if conditions.is_empty() {
    return "1=1".to_owned();
  }

  let parts: Vec<String> = conditions
    .iter()
    .filter_map(|condition| render_condition(condition, backend, params, index))
    .collect();

  if parts.is_empty() {
    "1=1".to_owned()
  } else {
    parts.join(" AND ")
  }
}

fn render_condition(
  condition: &Condition,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> Option<String> {
  match condition {
    Condition::Predicate(pred) => {
      render_field_filter(&pred.field, &pred.filter, backend, params, index)
    }
    Condition::And(filters) => {
      let parts: Vec<String> = filters
        .iter()
        .map(|filter| {
          render_conditions(&filter.conditions, backend, params, index)
        })
        .collect();
      if parts.is_empty() {
        None
      } else {
        Some(format!("({})", parts.join(" AND ")))
      }
    }
    Condition::Or(filters) => {
      let parts: Vec<String> = filters
        .iter()
        .map(|filter| {
          render_conditions(&filter.conditions, backend, params, index)
        })
        .collect();
      if parts.is_empty() {
        None
      } else {
        Some(format!("({})", parts.join(" OR ")))
      }
    }
    Condition::Not(filter) => {
      let inner = render_conditions(&filter.conditions, backend, params, index);
      Some(format!("NOT ({inner})"))
    }
    Condition::Relation(relation) => {
      let relation_sql =
        render_conditions(&relation.filter.conditions, backend, params, index);
      let prefix = match relation.op {
        RelationFilterOp::Some | RelationFilterOp::Is => "EXISTS",
        RelationFilterOp::None | RelationFilterOp::IsNot => "NOT EXISTS",
        RelationFilterOp::Every => "NOT EXISTS",
      };
      Some(format!("{prefix} (SELECT 1 WHERE {relation_sql})"))
    }
  }
}

fn render_field_filter(
  field: &str,
  filter: &FieldFilter,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> Option<String> {
  if !is_safe_identifier(field) {
    return Some("1=0".to_owned());
  }

  let rendered = match filter {
    FieldFilter::Null => format!("{field} IS NULL"),
    FieldFilter::NotNull => format!("{field} IS NOT NULL"),
    FieldFilter::String(filter) => {
      render_string_filter(field, filter, backend, params, index)
    }
    FieldFilter::Number(filter) => {
      render_number_filter(field, filter, backend, params, index)
    }
    FieldFilter::Bool(filter) => {
      render_bool_filter(field, filter, backend, params, index)
    }
    FieldFilter::Uuid(filter) => {
      render_uuid_filter(field, filter, backend, params, index)
    }
    FieldFilter::DateTime(filter) => {
      render_datetime_filter(field, filter, backend, params, index)
    }
    FieldFilter::Json(filter) => {
      render_json_filter(field, filter, backend, params, index)
    }
  };
  Some(rendered)
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

fn render_string_filter(
  field: &str,
  filter: &StringFilter,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match filter {
    StringFilter::Equals(v) => {
      binary(field, "=", v.clone(), backend, params, index)
    }
    StringFilter::NotEquals(v) => {
      binary(field, "!=", v.clone(), backend, params, index)
    }
    StringFilter::Like(v) => {
      binary(field, "LIKE", v.clone(), backend, params, index)
    }
    StringFilter::NotLike(v) => {
      binary(field, "NOT LIKE", v.clone(), backend, params, index)
    }
    StringFilter::Contains(v) => {
      binary(field, "LIKE", format!("%{v}%"), backend, params, index)
    }
    StringFilter::NotContains(v) => {
      binary(field, "NOT LIKE", format!("%{v}%"), backend, params, index)
    }
    StringFilter::StartsWith(v) => {
      binary(field, "LIKE", format!("{v}%"), backend, params, index)
    }
    StringFilter::EndsWith(v) => {
      binary(field, "LIKE", format!("%{v}"), backend, params, index)
    }
    StringFilter::GreaterThan(v) => {
      binary(field, ">", v.clone(), backend, params, index)
    }
    StringFilter::GreaterThanOrEquals(v) => {
      binary(field, ">=", v.clone(), backend, params, index)
    }
    StringFilter::LessThan(v) => {
      binary(field, "<", v.clone(), backend, params, index)
    }
    StringFilter::LessThanOrEquals(v) => {
      binary(field, "<=", v.clone(), backend, params, index)
    }
    StringFilter::In(values) => {
      in_list(field, values.clone(), true, backend, params, index)
    }
    StringFilter::NotIn(values) => {
      in_list(field, values.clone(), false, backend, params, index)
    }
    StringFilter::IsNull => format!("{field} IS NULL"),
    StringFilter::IsNotNull => format!("{field} IS NOT NULL"),
  }
}

fn render_number_filter(
  field: &str,
  filter: &NumberFilter,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match filter {
    NumberFilter::Equals(v) => binary(field, "=", *v, backend, params, index),
    NumberFilter::NotEquals(v) => {
      binary(field, "!=", *v, backend, params, index)
    }
    NumberFilter::GreaterThan(v) => {
      binary(field, ">", *v, backend, params, index)
    }
    NumberFilter::GreaterThanOrEquals(v) => {
      binary(field, ">=", *v, backend, params, index)
    }
    NumberFilter::LessThan(v) => binary(field, "<", *v, backend, params, index),
    NumberFilter::LessThanOrEquals(v) => {
      binary(field, "<=", *v, backend, params, index)
    }
    NumberFilter::In(values) => {
      in_list(field, values.clone(), true, backend, params, index)
    }
    NumberFilter::NotIn(values) => {
      in_list(field, values.clone(), false, backend, params, index)
    }
    NumberFilter::IsNull => format!("{field} IS NULL"),
    NumberFilter::IsNotNull => format!("{field} IS NOT NULL"),
  }
}

fn render_bool_filter(
  field: &str,
  filter: &BoolFilter,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match filter {
    BoolFilter::Equals(v) => binary(field, "=", *v, backend, params, index),
    BoolFilter::NotEquals(v) => binary(field, "!=", *v, backend, params, index),
    BoolFilter::In(values) => {
      in_list(field, values.clone(), true, backend, params, index)
    }
    BoolFilter::NotIn(values) => {
      in_list(field, values.clone(), false, backend, params, index)
    }
    BoolFilter::IsNull => format!("{field} IS NULL"),
    BoolFilter::IsNotNull => format!("{field} IS NOT NULL"),
  }
}

fn render_uuid_filter(
  field: &str,
  filter: &UuidFilter,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match filter {
    UuidFilter::Equals(v) => binary(field, "=", *v, backend, params, index),
    UuidFilter::NotEquals(v) => binary(field, "!=", *v, backend, params, index),
    UuidFilter::In(values) => {
      in_list(field, values.clone(), true, backend, params, index)
    }
    UuidFilter::NotIn(values) => {
      in_list(field, values.clone(), false, backend, params, index)
    }
    UuidFilter::IsNull => format!("{field} IS NULL"),
    UuidFilter::IsNotNull => format!("{field} IS NOT NULL"),
  }
}

fn render_datetime_filter(
  field: &str,
  filter: &DateTimeFilter,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match filter {
    DateTimeFilter::Equals(v) => binary(field, "=", *v, backend, params, index),
    DateTimeFilter::NotEquals(v) => {
      binary(field, "!=", *v, backend, params, index)
    }
    DateTimeFilter::GreaterThan(v) => {
      binary(field, ">", *v, backend, params, index)
    }
    DateTimeFilter::GreaterThanOrEquals(v) => {
      binary(field, ">=", *v, backend, params, index)
    }
    DateTimeFilter::LessThan(v) => {
      binary(field, "<", *v, backend, params, index)
    }
    DateTimeFilter::LessThanOrEquals(v) => {
      binary(field, "<=", *v, backend, params, index)
    }
    DateTimeFilter::In(values) => {
      in_list(field, values.clone(), true, backend, params, index)
    }
    DateTimeFilter::NotIn(values) => {
      in_list(field, values.clone(), false, backend, params, index)
    }
    DateTimeFilter::IsNull => format!("{field} IS NULL"),
    DateTimeFilter::IsNotNull => format!("{field} IS NOT NULL"),
  }
}

fn render_json_filter(
  field: &str,
  filter: &JsonFilter,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match filter {
    JsonFilter::Equals(v) => {
      binary(field, "=", v.clone(), backend, params, index)
    }
    JsonFilter::NotEquals(v) => {
      binary(field, "!=", v.clone(), backend, params, index)
    }
    JsonFilter::Contains(v) => match backend {
      SqlBackend::Postgres => {
        binary(field, "@>", v.clone(), backend, params, index)
      }
      SqlBackend::MySql | SqlBackend::Sqlite => {
        let placeholder =
          push_param(params, ScalarValue::Json(v.clone()), backend, index);
        format!("JSON_CONTAINS({field}, {placeholder})")
      }
    },
    JsonFilter::NotContains(v) => match backend {
      SqlBackend::Postgres => {
        let contains = binary(field, "@>", v.clone(), backend, params, index);
        format!("NOT ({contains})")
      }
      SqlBackend::MySql | SqlBackend::Sqlite => {
        let placeholder =
          push_param(params, ScalarValue::Json(v.clone()), backend, index);
        format!("NOT JSON_CONTAINS({field}, {placeholder})")
      }
    },
    JsonFilter::HasKey(key) => match backend {
      SqlBackend::Postgres => {
        let placeholder =
          push_param(params, ScalarValue::String(key.clone()), backend, index);
        format!("{field} ? {placeholder}")
      }
      SqlBackend::MySql | SqlBackend::Sqlite => {
        let json_path = format!("$.{key}");
        let placeholder =
          push_param(params, ScalarValue::String(json_path), backend, index);
        format!("JSON_EXTRACT({field}, {placeholder}) IS NOT NULL")
      }
    },
    JsonFilter::HasAnyKey(keys) => {
      render_json_has_keys(field, keys, true, backend, params, index)
    }
    JsonFilter::HasEveryKey(keys) => {
      render_json_has_keys(field, keys, false, backend, params, index)
    }
    JsonFilter::PathEquals { path, value } => {
      let accessor = render_json_path(field, path, backend, params, index);
      let placeholder =
        push_param(params, ScalarValue::Json(value.clone()), backend, index);
      format!("{accessor} = {placeholder}")
    }
    JsonFilter::PathNotEquals { path, value } => {
      let accessor = render_json_path(field, path, backend, params, index);
      let placeholder =
        push_param(params, ScalarValue::Json(value.clone()), backend, index);
      format!("{accessor} != {placeholder}")
    }
    JsonFilter::PathLike { path, value } => {
      let accessor = render_json_text_path(field, path, backend, params, index);
      let placeholder =
        push_param(params, ScalarValue::String(value.clone()), backend, index);
      format!("{accessor} LIKE {placeholder}")
    }
    JsonFilter::PathNotLike { path, value } => {
      let accessor = render_json_text_path(field, path, backend, params, index);
      let placeholder =
        push_param(params, ScalarValue::String(value.clone()), backend, index);
      format!("{accessor} NOT LIKE {placeholder}")
    }
    JsonFilter::PathStartsWith { path, value } => {
      let accessor = render_json_text_path(field, path, backend, params, index);
      let placeholder = push_param(
        params,
        ScalarValue::String(format!("{value}%")),
        backend,
        index,
      );
      format!("{accessor} LIKE {placeholder}")
    }
    JsonFilter::PathEndsWith { path, value } => {
      let accessor = render_json_text_path(field, path, backend, params, index);
      let placeholder = push_param(
        params,
        ScalarValue::String(format!("%{value}")),
        backend,
        index,
      );
      format!("{accessor} LIKE {placeholder}")
    }
    JsonFilter::PathContains { path, value } => {
      let accessor = render_json_text_path(field, path, backend, params, index);
      let placeholder = push_param(
        params,
        ScalarValue::String(format!("%{value}%")),
        backend,
        index,
      );
      format!("{accessor} LIKE {placeholder}")
    }
    JsonFilter::IsNull => format!("{field} IS NULL"),
    JsonFilter::IsNotNull => format!("{field} IS NOT NULL"),
  }
}

fn render_json_path(
  field: &str,
  path: &[String],
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match backend {
    SqlBackend::Postgres => {
      let path = format!("{{{}}}", path.join(","));
      let placeholder =
        push_param(params, ScalarValue::String(path), backend, index);
      format!("{field} #>> {placeholder}")
    }
    SqlBackend::MySql | SqlBackend::Sqlite => {
      let path = format!("$.{}", path.join("."));
      let placeholder =
        push_param(params, ScalarValue::String(path), backend, index);
      format!("JSON_EXTRACT({field}, {placeholder})")
    }
  }
}

fn render_json_text_path(
  field: &str,
  path: &[String],
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  match backend {
    SqlBackend::Postgres => {
      render_json_path(field, path, backend, params, index)
    }
    SqlBackend::MySql | SqlBackend::Sqlite => {
      let path = format!("$.{}", path.join("."));
      let placeholder =
        push_param(params, ScalarValue::String(path), backend, index);
      format!("JSON_UNQUOTE(JSON_EXTRACT({field}, {placeholder}))")
    }
  }
}

fn render_json_has_keys(
  field: &str,
  keys: &[String],
  any: bool,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  if keys.is_empty() {
    return "1=1".to_owned();
  }

  let joiner = if any { " OR " } else { " AND " };
  let checks = keys
    .iter()
    .map(|key| match backend {
      SqlBackend::Postgres => {
        let placeholder =
          push_param(params, ScalarValue::String(key.clone()), backend, index);
        format!("{field} ? {placeholder}")
      }
      SqlBackend::MySql | SqlBackend::Sqlite => {
        let placeholder = push_param(
          params,
          ScalarValue::String(format!("$.{key}")),
          backend,
          index,
        );
        format!("JSON_EXTRACT({field}, {placeholder}) IS NOT NULL")
      }
    })
    .collect::<Vec<_>>();

  format!("({})", checks.join(joiner))
}

fn binary<V: Into<ScalarValue>>(
  field: &str,
  op: &str,
  value: V,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  let placeholder = push_param(params, value.into(), backend, index);
  format!("{field} {op} {placeholder}")
}

fn in_list<V: Into<ScalarValue>>(
  field: &str,
  values: Vec<V>,
  include: bool,
  backend: SqlBackend,
  params: &mut Vec<ScalarValue>,
  index: &mut usize,
) -> String {
  if values.is_empty() {
    return if include {
      "1=0".to_owned()
    } else {
      "1=1".to_owned()
    };
  }

  let placeholders: Vec<String> = values
    .into_iter()
    .map(|value| push_param(params, value.into(), backend, index))
    .collect();
  let op = if include { "IN" } else { "NOT IN" };
  format!("{field} {op} ({})", placeholders.join(", "))
}

fn push_param(
  params: &mut Vec<ScalarValue>,
  value: ScalarValue,
  backend: SqlBackend,
  index: &mut usize,
) -> String {
  params.push(value);
  *index += 1;
  match backend {
    SqlBackend::Postgres => format!("${index}"),
    SqlBackend::MySql | SqlBackend::Sqlite => "?".to_owned(),
  }
}
