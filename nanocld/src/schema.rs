// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "node_modes"))]
    pub struct NodeModes;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "resource_kind"))]
    pub struct ResourceKind;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "ssh_auth_modes"))]
    pub struct SshAuthModes;
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
    use diesel::sql_types::*;
    use super::sql_types::NodeModes;
    use super::sql_types::SshAuthModes;

    nodes (name) {
        name -> Varchar,
        mode -> NodeModes,
        ip_address -> Varchar,
        ssh_auth_mode -> SshAuthModes,
        ssh_user -> Varchar,
        ssh_credential -> Varchar,
    }
}

diesel::table! {
    resource_configs (key) {
        key -> Uuid,
        resource_key -> Varchar,
        config -> Jsonb,
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

diesel::allow_tables_to_appear_in_same_query!(
    cargo_configs,
    cargoes,
    namespaces,
    nodes,
    resource_configs,
    resources,
);
