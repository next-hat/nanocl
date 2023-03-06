// @generated automatically by Diesel CLI.

diesel::table! {
    cargo_configs (key) {
        key -> Uuid,
        cargo_key -> Varchar,
        version -> Varchar,
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
        node_name -> Varchar,
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
    node_group_links (rowid) {
        node_name -> Varchar,
        node_group_name -> Varchar,
        rowid -> Int8,
    }
}

diesel::table! {
    node_groups (name) {
        name -> Varchar,
    }
}

diesel::table! {
    nodes (name) {
        name -> Varchar,
        ip_address -> Varchar,
    }
}

diesel::table! {
    resource_configs (key) {
        key -> Uuid,
        resource_key -> Varchar,
        version -> Varchar,
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
diesel::joinable!(node_group_links -> node_groups (node_group_name));
diesel::joinable!(node_group_links -> nodes (node_name));
diesel::joinable!(resources -> resource_configs (config_key));

diesel::allow_tables_to_appear_in_same_query!(
  cargo_configs,
  cargoes,
  metrics,
  namespaces,
  node_group_links,
  node_groups,
  nodes,
  resource_configs,
  resources,
);
