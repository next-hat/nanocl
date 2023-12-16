// @generated automatically by Diesel CLI.

diesel::table! {
    cargo_specs (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        cargo_key -> Varchar,
        version -> Varchar,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    cargoes (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        name -> Varchar,
        spec_key -> Uuid,
        namespace_name -> Varchar,
    }
}

diesel::table! {
    http_metrics (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        expire_at -> Timestamptz,
        date_gmt -> Timestamptz,
        status -> Int8,
        bytes_sent -> Int8,
        content_length -> Int8,
        body_bytes_sent -> Int8,
        request_time -> Float8,
        node_name -> Varchar,
        uri -> Varchar,
        host -> Varchar,
        remote_addr -> Varchar,
        realip_remote_addr -> Varchar,
        server_protocol -> Varchar,
        request_method -> Varchar,
        proxy_host -> Nullable<Varchar>,
        upstream_addr -> Nullable<Varchar>,
        query_string -> Nullable<Varchar>,
        request_body -> Nullable<Varchar>,
        content_type -> Nullable<Varchar>,
        http_user_agent -> Nullable<Varchar>,
        http_referrer -> Nullable<Varchar>,
        http_accept_language -> Nullable<Varchar>,
    }
}

diesel::table! {
    jobs (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    metrics (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        expire_at -> Timestamptz,
        node_name -> Varchar,
        kind -> Varchar,
        data -> Jsonb,
    }
}

diesel::table! {
    namespaces (name) {
        name -> Varchar,
        created_at -> Timestamptz,
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
    processes (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        name -> Varchar,
        kind -> Varchar,
        data -> Jsonb,
        node_key -> Varchar,
        kind_key -> Varchar,
    }
}

diesel::table! {
    resource_kind_versions (resource_kind_name, version) {
        resource_kind_name -> Varchar,
        created_at -> Timestamptz,
        version -> Varchar,
        schema -> Nullable<Jsonb>,
        url -> Nullable<Varchar>,
    }
}

diesel::table! {
    resource_kinds (name) {
        name -> Varchar,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    resource_specs (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        resource_key -> Varchar,
        version -> Varchar,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    resources (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        kind -> Varchar,
        spec_key -> Uuid,
    }
}

diesel::table! {
    secrets (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        kind -> Varchar,
        immutable -> Bool,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    stream_metrics (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        expire_at -> Timestamptz,
        date_gmt -> Timestamptz,
        remote_addr -> Varchar,
        upstream_addr -> Varchar,
        protocol -> Nullable<Varchar>,
        status -> Int8,
        session_time -> Varchar,
        bytes_sent -> Int8,
        bytes_received -> Int8,
        upstream_bytes_sent -> Int8,
        upstream_bytes_received -> Int8,
        upstream_connect_time -> Varchar,
        node_name -> Varchar,
    }
}

diesel::table! {
    vm_images (name) {
        name -> Varchar,
        created_at -> Timestamptz,
        kind -> Varchar,
        path -> Varchar,
        format -> Varchar,
        size_actual -> Int8,
        size_virtual -> Int8,
        parent -> Nullable<Varchar>,
    }
}

diesel::table! {
    vm_specs (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        vm_key -> Varchar,
        version -> Varchar,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    vms (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        name -> Varchar,
        spec_key -> Uuid,
        namespace_name -> Varchar,
    }
}

diesel::joinable!(cargoes -> cargo_specs (spec_key));
diesel::joinable!(cargoes -> namespaces (namespace_name));
diesel::joinable!(node_group_links -> node_groups (node_group_name));
diesel::joinable!(node_group_links -> nodes (node_name));
diesel::joinable!(resource_kind_versions -> resource_kinds (resource_kind_name));
diesel::joinable!(resources -> resource_specs (spec_key));
diesel::joinable!(vms -> namespaces (namespace_name));
diesel::joinable!(vms -> vm_specs (spec_key));

diesel::allow_tables_to_appear_in_same_query!(
    cargo_specs,
    cargoes,
    http_metrics,
    jobs,
    metrics,
    namespaces,
    node_group_links,
    node_groups,
    nodes,
    processes,
    resource_kind_versions,
    resource_kinds,
    resource_specs,
    resources,
    secrets,
    stream_metrics,
    vm_images,
    vm_specs,
    vms,
);
