use prismar::{FieldFilter, ModelFilter, SqlBackend, StringFilter};

#[test]
fn blocks_unsafe_identifier_in_rendering() {
  let filter = ModelFilter::empty().predicate(
    "name; DROP TABLE users; --",
    FieldFilter::String(StringFilter::Equals("api".into())),
  );

  let rendered = filter.render(SqlBackend::Postgres);
  assert_eq!(rendered.sql, "1=0");
}
