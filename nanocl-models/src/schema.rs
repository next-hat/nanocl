pub mod sql_types {
  #[derive(diesel::sql_types::SqlType)]
  #[diesel(postgres_type(name = "resource_kind"))]
  pub struct ResourceKind;
}
