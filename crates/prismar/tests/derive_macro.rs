use chrono::NaiveDateTime;
use prismar::PrismarModel;

#[derive(Debug, Clone, PrismarModel)]
#[allow(dead_code)]
struct SpecDb {
  key: uuid::Uuid,
  version: String,
}

#[derive(Debug, Clone, PrismarModel)]
#[allow(dead_code)]
struct CargoDbLike {
  key: String,
  created_at: NaiveDateTime,
  name: String,
  spec_key: uuid::Uuid,
  status_key: String,
  namespace_name: String,
  metadata: Option<serde_json::Value>,
  #[prismar(relation)]
  spec: Option<SpecDb>,
}

#[test]
fn generated_filter_is_type_safe() {
  let spec_filter = SpecDbFilter::new().version_where(|f| f.starts_with("v"));

  let filter = CargoDbLikeFilter::new()
    .name_where(|f| f.contains("gateway").not_contains("deprecated"))
    .spec_key_where(|f| f.eq(uuid::Uuid::nil()))
    .metadata_is_null()
    .spec_some(spec_filter)
    .build();

  let rendered = filter.render(prismar::SqlBackend::Postgres);
  assert!(rendered.sql.contains("name LIKE"));
  assert!(rendered.sql.contains("name NOT LIKE"));
  assert!(rendered.sql.contains("spec_key ="));
  assert!(rendered.sql.contains("metadata IS NULL"));
  assert!(rendered.sql.contains("EXISTS (SELECT 1 WHERE"));
}
