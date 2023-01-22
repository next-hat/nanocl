// @generated automatically by Diesel CLI.

pub mod sql_types {
  #[derive(diesel::sql_types::SqlType)]
  #[diesel(postgres_type(name = "resource_kind"))]
  pub struct ResourceKind;
}

diesel::table! {
    cargo_configs (key) {
        key -> Uuid,
        cargo_key -> Varchar,
        config -> Jsonb,
    }
}

diesel::table! {
    cargoes (key) {
        key -> Varchar,
        name -> Varchar,
        config_key -> Uuid,
        namespace_name -> Varchar,
    }
}

diesel::table! {
    namespaces (name) {
        name -> Varchar,
    }
}

diesel::table! {
    resource_configs (key) {
        key -> Uuid,
        resource_key -> Varchar,
        data -> Jsonb,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::ResourceKind;

    resources (key) {
        key -> Varchar,
        kind -> ResourceKind,
        config_key -> Uuid,
    }
}

joinable!(cargoes -> cargo_configs(config_key));
joinable!(resources -> resource_configs(config_key));

diesel::allow_tables_to_appear_in_same_query!(
  cargo_configs,
  cargoes,
  namespaces,
  resource_configs,
  resources,
);
