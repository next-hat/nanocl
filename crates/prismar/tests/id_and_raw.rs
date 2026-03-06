use prismar::{
  DeleteByIdArgs, IdSelector, ModelFilter, OrderDirection, RawSqlBuilder,
  ReadByIdArgs, SqlBackend,
};

#[test]
fn supports_read_and_delete_by_various_id_types() {
  let uuid = uuid::Uuid::nil();
  let read_filter = ModelFilter::read_by_id("key", uuid);
  let rendered = read_filter.render(SqlBackend::Postgres);
  assert!(rendered.sql.contains("key = $1"));

  let delete_args = ModelFilter::delete_by_id("id", 42_i64);
  let rendered_delete = delete_args.r#where.render(SqlBackend::Postgres);
  assert!(rendered_delete.sql.contains("id = $1"));

  let args = ReadByIdArgs {
    id: IdSelector::new("namespace_name", "system"),
  };
  let parsed = args.to_filter().unwrap();
  assert!(parsed.render(SqlBackend::Postgres).sql.contains("namespace_name = $1"));

  let delete = DeleteByIdArgs {
    id: IdSelector::new("id", 7_u64),
  };
  assert!(delete.to_filter().is_ok());
}

#[test]
fn raw_builder_is_parameterized_and_safe() {
  let query = RawSqlBuilder::new(
    SqlBackend::Postgres,
    "SELECT namespace_name, count(*) FROM cargoes",
  )
  .unwrap()
  .filter(ModelFilter::read_by_id("namespace_name", "prod"))
  .order_by("namespace_name", OrderDirection::Asc)
  .unwrap()
  .limit(10)
  .offset(5)
  .build()
  .unwrap();

  assert!(query.sql.contains("WHERE namespace_name = $1"));
  assert!(query.sql.contains("ORDER BY namespace_name ASC"));
  assert!(query.sql.contains("LIMIT $2"));
  assert!(query.sql.contains("OFFSET $3"));
  assert_eq!(query.params.len(), 3);
}

#[test]
fn raw_builder_rejects_unsafe_identifiers() {
  let result = RawSqlBuilder::new(SqlBackend::Postgres, "SELECT 1")
    .unwrap()
    .order_by("name; DROP TABLE users; --", OrderDirection::Asc);
  assert!(result.is_err());

  let id = IdSelector::new("id; DELETE FROM x", 1_u64);
  assert!(id.to_filter().is_err());
}
