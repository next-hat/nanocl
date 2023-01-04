// @generated automatically by Diesel CLI.

pub mod sql_types {
  // #[derive(diesel::sql_types::SqlType)]
  // #[diesel(postgres_type(name = "node_modes"))]
  // pub struct NodeModes;

  // #[derive(diesel::sql_types::SqlType)]
  // #[diesel(postgres_type(name = "proxy_template_modes"))]
  // pub struct ProxyTemplateModes;

  // #[derive(diesel::sql_types::SqlType)]
  // #[diesel(postgres_type(name = "ssh_auth_modes"))]
  // pub struct SshAuthModes;
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

// diesel::table! {
//     nginx_logs (key) {
//         key -> Uuid,
//         date_gmt -> Timestamptz,
//         uri -> Varchar,
//         host -> Varchar,
//         remote_addr -> Varchar,
//         realip_remote_addr -> Varchar,
//         server_protocol -> Varchar,
//         request_method -> Varchar,
//         content_length -> Int8,
//         status -> Int8,
//         request_time -> Float8,
//         body_bytes_sent -> Int8,
//         proxy_host -> Nullable<Varchar>,
//         upstream_addr -> Nullable<Varchar>,
//         query_string -> Nullable<Varchar>,
//         request_body -> Nullable<Varchar>,
//         content_type -> Nullable<Varchar>,
//         http_user_agent -> Nullable<Varchar>,
//         http_referrer -> Nullable<Varchar>,
//         http_accept_language -> Nullable<Varchar>,
//     }
// }

// diesel::table! {
//     use diesel::sql_types::*;
//     use super::sql_types::NodeModes;
//     use super::sql_types::SshAuthModes;

//     nodes (name) {
//         name -> Varchar,
//         mode -> NodeModes,
//         ip_address -> Varchar,
//         ssh_auth_mode -> SshAuthModes,
//         ssh_user -> Varchar,
//         ssh_credential -> Varchar,
//     }
// }

// diesel::table! {
//     use diesel::sql_types::*;
//     use super::sql_types::ProxyTemplateModes;

//     proxy_templates (name) {
//         name -> Varchar,
//         mode -> ProxyTemplateModes,
//         content -> Text,
//     }
// }

diesel::allow_tables_to_appear_in_same_query!(
  cargo_configs,
  cargoes,
  namespaces,
  // nginx_logs,
  // nodes,
  // proxy_templates,
);
