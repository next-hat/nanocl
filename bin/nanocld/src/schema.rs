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
    metrics (key) {
        key -> Uuid,
        kind -> Text,
        data -> Jsonb,
        created_at -> Timestamptz,
        expire_at -> Timestamptz,
    }
}

diesel::table! {
    namespaces (name) {
        name -> Varchar,
    }
}

diesel::table! {
    nodes (name) {
        name -> Varchar,
        mode -> Text,
        labels -> Jsonb,
        ip_address -> Varchar,
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
    resources (key) {
        key -> Varchar,
        kind -> Varchar,
        config_key -> Uuid,
    }
}

diesel::joinable!(cargoes -> cargo_configs (config_key));
diesel::joinable!(cargoes -> namespaces (namespace_name));
diesel::joinable!(resources -> resource_configs (config_key));

diesel::allow_tables_to_appear_in_same_query!(
    cargo_configs,
    cargoes,
    metrics,
    namespaces,
    nodes,
    resource_configs,
    resources,
);
