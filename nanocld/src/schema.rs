// @generated automatically by Diesel CLI.

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

joinable!(cargoes -> cargo_configs(config_key));

diesel::allow_tables_to_appear_in_same_query!(
  cargo_configs,
  cargoes,
  namespaces,
);
