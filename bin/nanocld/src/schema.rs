// @generated automatically by Diesel CLI.

diesel::table! {
    cargoes (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        name -> Varchar,
        spec_key -> Uuid,
        status_key -> Varchar,
        namespace_name -> Varchar,
    }
}

diesel::table! {
    events (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        expires_at -> Timestamptz,
        reporting_node -> Varchar,
        reporting_controller -> Varchar,
        kind -> Varchar,
        action -> Varchar,
        reason -> Varchar,
        note -> Nullable<Varchar>,
        actor -> Nullable<Jsonb>,
        related -> Nullable<Jsonb>,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    jobs (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        status_key -> Varchar,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    metrics (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        expires_at -> Timestamptz,
        node_name -> Varchar,
        kind -> Varchar,
        data -> Jsonb,
        note -> Nullable<Varchar>,
    }
}

diesel::table! {
    namespaces (name) {
        name -> Varchar,
        created_at -> Timestamptz,
        metadata -> Nullable<Jsonb>,
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
        created_at -> Timestamptz,
        ip_address -> Inet,
        endpoint -> Varchar,
        version -> Varchar,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    object_process_statuses (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        wanted -> Varchar,
        prev_wanted -> Varchar,
        actual -> Varchar,
        prev_actual -> Varchar,
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
        node_name -> Varchar,
        kind_key -> Varchar,
    }
}

diesel::table! {
    resource_kinds (name) {
        name -> Varchar,
        created_at -> Timestamptz,
        spec_key -> Uuid,
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
    specs (key) {
        key -> Uuid,
        created_at -> Timestamptz,
        kind_name -> Varchar,
        kind_key -> Varchar,
        version -> Varchar,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    vm_images (name) {
        name -> Varchar,
        node_name -> Varchar,
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
    vms (key) {
        key -> Varchar,
        name -> Varchar,
        created_at -> Timestamptz,
        namespace_name -> Varchar,
        status_key -> Varchar,
        spec_key -> Uuid,
    }
}

diesel::joinable!(cargoes -> namespaces (namespace_name));
diesel::joinable!(cargoes -> object_process_statuses (status_key));
diesel::joinable!(cargoes -> specs (spec_key));
diesel::joinable!(jobs -> object_process_statuses (status_key));
diesel::joinable!(node_group_links -> node_groups (node_group_name));
diesel::joinable!(node_group_links -> nodes (node_name));
diesel::joinable!(processes -> nodes (node_name));
diesel::joinable!(resource_kinds -> specs (spec_key));
diesel::joinable!(resources -> specs (spec_key));
diesel::joinable!(vm_images -> nodes (node_name));
diesel::joinable!(vms -> namespaces (namespace_name));
diesel::joinable!(vms -> object_process_statuses (status_key));
diesel::joinable!(vms -> specs (spec_key));

diesel::allow_tables_to_appear_in_same_query!(
  cargoes,
  events,
  jobs,
  metrics,
  namespaces,
  node_group_links,
  node_groups,
  nodes,
  object_process_statuses,
  processes,
  resource_kinds,
  resources,
  secrets,
  specs,
  vm_images,
  vms,
);
