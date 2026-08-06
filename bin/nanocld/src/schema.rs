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
    cargo_replica_processes (key) {
        key -> Uuid,
        replica_key -> Uuid,
        process_key -> Nullable<Varchar>,
        container_name -> Varchar,
        role -> Varchar,
        essential -> Bool,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    cargo_replicas (key) {
        key -> Uuid,
        cargo_key -> Varchar,
        ordinal -> Int4,
        node_name -> Nullable<Varchar>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    distributed_mutexes (key) {
        key -> Uuid,
        action -> Text,
        resource_kind -> Text,
        resource_key -> Text,
        node_key -> Text,
        acquired_by -> Text,
        acquired_at -> Timestamptz,
        expires_at -> Timestamptz,
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
    networks (key) {
        key -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        name -> Varchar,
        node_name -> Varchar,
        data -> Jsonb,
        metadata -> Nullable<Jsonb>,
    }
}

diesel::table! {
    nodes (name) {
        name -> Varchar,
        created_at -> Timestamptz,
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
        health -> Varchar,
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
diesel::joinable!(cargo_replica_processes -> cargo_replicas (replica_key));
diesel::joinable!(cargo_replica_processes -> processes (process_key));
diesel::joinable!(cargo_replicas -> cargoes (cargo_key));
diesel::joinable!(cargo_replicas -> nodes (node_name));
diesel::joinable!(jobs -> object_process_statuses (status_key));
diesel::joinable!(networks -> nodes (node_name));
diesel::joinable!(processes -> nodes (node_name));
diesel::joinable!(resource_kinds -> specs (spec_key));
diesel::joinable!(resources -> specs (spec_key));
diesel::joinable!(vm_images -> nodes (node_name));
diesel::joinable!(vms -> namespaces (namespace_name));
diesel::joinable!(vms -> object_process_statuses (status_key));
diesel::joinable!(vms -> specs (spec_key));

diesel::allow_tables_to_appear_in_same_query!(
  cargoes,
  cargo_replica_processes,
  cargo_replicas,
  distributed_mutexes,
  events,
  jobs,
  metrics,
  namespaces,
  networks,
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
