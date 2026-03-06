use chrono::NaiveDate;
use prismar::{
  DateTimeFilter, FieldFilter, JsonFilter, ModelFilter, NumberFilter, SqlBackend,
  StringFilter,
};

#[test]
fn renders_prisma_like_sql_for_postgres() {
  let created_at = NaiveDate::from_ymd_opt(2025, 1, 1)
    .unwrap()
    .and_hms_opt(0, 0, 0)
    .unwrap();

  let filter = ModelFilter::empty()
    .predicate("name", FieldFilter::String(StringFilter::Contains("api".into())))
    .predicate("created_at", FieldFilter::DateTime(DateTimeFilter::GreaterThanOrEquals(created_at)))
    .predicate("replicas", FieldFilter::Number(NumberFilter::GreaterThan(2.0)))
    .predicate(
      "data",
      FieldFilter::Json(JsonFilter::PathEquals {
        path: vec!["Config".into(), "Labels".into(), "io.nanocl.role".into()],
        value: serde_json::json!("edge"),
      }),
    );

  let rendered = filter.render(SqlBackend::Postgres);

  assert!(rendered.sql.contains("name LIKE"));
  assert!(rendered.sql.contains("created_at >="));
  assert!(rendered.sql.contains("replicas >"));
  assert!(rendered.sql.contains("data #>>"));
  assert_eq!(rendered.params.len(), 5);
}

#[test]
fn renders_json_contains_for_sqlite() {
  let filter = ModelFilter::empty().predicate(
    "payload",
    FieldFilter::Json(JsonFilter::Contains(serde_json::json!({"hello": true}))),
  );

  let rendered = filter.render(SqlBackend::Sqlite);

  assert!(rendered.sql.contains("JSON_CONTAINS(payload, ?)"));
  assert_eq!(rendered.params.len(), 1);
}

#[test]
fn renders_json_path_like_and_not_like() {
  let filter = ModelFilter::empty()
    .predicate(
      "data",
      FieldFilter::Json(JsonFilter::PathLike {
        path: vec!["Config".into(), "Labels".into(), "team".into()],
        value: "%core%".into(),
      }),
    )
    .predicate(
      "data",
      FieldFilter::Json(JsonFilter::PathNotLike {
        path: vec!["Config".into(), "Labels".into(), "env".into()],
        value: "%dev%".into(),
      }),
    );

  let pg = filter.render(SqlBackend::Postgres);
  assert!(pg.sql.contains("data #>>"));
  assert!(pg.sql.contains("LIKE"));
  assert!(pg.sql.contains("NOT LIKE"));

  let sqlite = filter.render(SqlBackend::Sqlite);
  assert!(sqlite.sql.contains("JSON_UNQUOTE(JSON_EXTRACT(data, ?)) LIKE ?"));
  assert!(
    sqlite
      .sql
      .contains("JSON_UNQUOTE(JSON_EXTRACT(data, ?)) NOT LIKE ?")
  );

  let mysql = filter.render(SqlBackend::MySql);
  assert!(mysql.sql.contains("JSON_UNQUOTE(JSON_EXTRACT(data, ?)) LIKE ?"));
  assert!(
    mysql
      .sql
      .contains("JSON_UNQUOTE(JSON_EXTRACT(data, ?)) NOT LIKE ?")
  );
}

#[test]
fn backend_placeholder_styles_are_respected() {
  let filter = ModelFilter::empty()
    .predicate("name", FieldFilter::String(StringFilter::Equals("api".into())));

  let pg = filter.render(SqlBackend::Postgres);
  assert!(pg.sql.contains("$1"));

  let sqlite = filter.render(SqlBackend::Sqlite);
  assert!(sqlite.sql.contains("?"));

  let mysql = filter.render(SqlBackend::MySql);
  assert!(mysql.sql.contains("?"));
}
