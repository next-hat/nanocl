use std::collections::{HashMap, HashSet};

use nanocl_error::io::IoResult;
use nanocl_stubs::{cargo::Cargo, process::Process};

use crate::{
  models::{
    CargoReplicaDb, CargoReplicaProcessDb, CargoReplicaProcessRole,
    ValidateRestartSlotOpts,
  },
  utils::container::cargo_compiler::LABEL_ROLE,
};

use super::{
  RuntimeSlot, SANDBOX_LOGICAL_NAME,
  identity::{is_rollout_process_name, process_identity},
  needs_sandbox, restart_topology_error,
};

#[derive(Debug)]
pub(super) struct CargoRestartPlan {
  pub(super) slots: Vec<RuntimeSlot>,
}

fn validate_restart_slot(
  opts: ValidateRestartSlotOpts<'_, '_>,
) -> IoResult<RuntimeSlot> {
  let ValidateRestartSlotOpts {
    cargo_key,
    replica,
    mapping,
    expected_role,
    expected_name,
    expected_essential,
    processes_by_key,
    stable_identities,
  } = opts;
  if mapping.role != expected_role
    || mapping.container_name != expected_name
    || mapping.essential != expected_essential
  {
    return Err(restart_topology_error(
      cargo_key,
      &format!(
        "replica {} has a contradictory {expected_role} mapping for {expected_name:?}",
        replica.key
      ),
    ));
  }
  let process_key = mapping.process_key.as_deref().ok_or_else(|| {
    restart_topology_error(
      cargo_key,
      &format!(
        "replica {} {expected_role} slot {expected_name:?} has no attached process",
        replica.key
      ),
    )
  })?;
  let process = processes_by_key.get(process_key).copied().ok_or_else(|| {
    restart_topology_error(
      cargo_key,
      &format!(
        "replica {} {expected_role} slot {expected_name:?} references missing process {process_key:?}",
        replica.key
      ),
    )
  })?;
  if is_rollout_process_name(&process.name) {
    return Err(restart_topology_error(
      cargo_key,
      &format!(
        "replica {} {expected_role} slot {expected_name:?} is attached to rollout process {:?}",
        replica.key, process.name
      ),
    ));
  }
  let identity = process_identity(process)?;
  if identity.replica_key != replica.key
    || identity.ordinal != replica.ordinal
    || identity.role != expected_role
    || identity.container_name != expected_name
    || identity.essential != expected_essential
  {
    return Err(restart_topology_error(
      cargo_key,
      &format!(
        "replica {} {expected_role} mapping for {expected_name:?} contradicts process {process_key:?} identity",
        replica.key
      ),
    ));
  }
  let active = stable_identities
    .get(&(replica.key, expected_role, expected_name.to_owned()))
    .map(Vec::as_slice)
    .unwrap_or_default();
  if active.len() != 1 || active[0].key != process_key {
    return Err(restart_topology_error(
      cargo_key,
      &format!(
        "replica {} {expected_role} slot {expected_name:?} has {} active processes instead of its one mapped process",
        replica.key,
        active.len()
      ),
    ));
  }
  Ok(RuntimeSlot {
    mapping: mapping.clone(),
    process: process.clone(),
    role: expected_role,
    container_name: expected_name.to_owned(),
    essential: expected_essential,
  })
}

/// Select the exact active application topology for an explicit Cargo
/// restart. Replica ordinals and authored application order determine Docker
/// restart order; init mappings and rollout artifacts never become targets.
pub(super) fn plan_cargo_restart(
  cargo: &Cargo,
  replicas: &[CargoReplicaDb],
  mappings: &[CargoReplicaProcessDb],
  processes: &[Process],
  local_node: &str,
) -> IoResult<CargoRestartPlan> {
  let cargo_key = &cargo.spec.cargo_key;
  let mut replicas = replicas.to_vec();
  replicas.sort_by_key(|replica| replica.ordinal);
  if replicas.len() != cargo.spec.replicas {
    return Err(restart_topology_error(
      cargo_key,
      &format!(
        "{} durable replicas do not match {} desired replicas",
        replicas.len(),
        cargo.spec.replicas
      ),
    ));
  }
  for (expected_ordinal, replica) in replicas.iter().enumerate() {
    if replica.cargo_key != *cargo_key
      || replica.ordinal != expected_ordinal as i32
      || replica.node_name.as_deref() != Some(local_node)
    {
      return Err(restart_topology_error(
        cargo_key,
        &format!(
          "replica {} / ordinal {} is not the expected local replica {expected_ordinal}",
          replica.key, replica.ordinal
        ),
      ));
    }
  }

  let processes_by_key = processes
    .iter()
    .map(|process| (process.key.as_str(), process))
    .collect::<HashMap<_, _>>();
  let mut stable_identities = HashMap::<
    (uuid::Uuid, CargoReplicaProcessRole, String),
    Vec<&Process>,
  >::new();
  for process in processes
    .iter()
    .filter(|process| !is_rollout_process_name(&process.name))
  {
    let identity = match process_identity(process) {
      Ok(identity) => identity,
      Err(error) => {
        let role = process
          .data
          .config
          .as_ref()
          .and_then(|config| config.labels.as_ref())
          .and_then(|labels| labels.get(LABEL_ROLE))
          .map(String::as_str);
        if matches!(role, Some("app" | "sandbox")) {
          return Err(restart_topology_error(
            cargo_key,
            &format!(
              "active process {} has invalid runtime identity: {error}",
              process.key
            ),
          ));
        }
        continue;
      }
    };
    stable_identities
      .entry((identity.replica_key, identity.role, identity.container_name))
      .or_default()
      .push(process);
  }

  let desired_app_names = cargo
    .spec
    .containers
    .iter()
    .map(|container| container.name.as_str())
    .collect::<HashSet<_>>();
  let replica_keys = replicas
    .iter()
    .map(|replica| replica.key)
    .collect::<HashSet<_>>();
  for ((replica_key, role, container_name), active) in &stable_identities {
    if !replica_keys.contains(replica_key) {
      if matches!(
        role,
        CargoReplicaProcessRole::App | CargoReplicaProcessRole::Sandbox
      ) {
        return Err(restart_topology_error(
          cargo_key,
          &format!(
            "unexpected active {role} process for unknown replica {replica_key}"
          ),
        ));
      }
      continue;
    }
    if *role == CargoReplicaProcessRole::App
      && !desired_app_names.contains(container_name.as_str())
    {
      return Err(restart_topology_error(
        cargo_key,
        &format!(
          "replica {replica_key} has unexpected active application {container_name:?}"
        ),
      ));
    }
    if *role == CargoReplicaProcessRole::Sandbox
      && container_name != SANDBOX_LOGICAL_NAME
    {
      return Err(restart_topology_error(
        cargo_key,
        &format!(
          "replica {replica_key} has unexpected active sandbox {container_name:?}"
        ),
      ));
    }
    if matches!(
      role,
      CargoReplicaProcessRole::App | CargoReplicaProcessRole::Sandbox
    ) && active.len() > 1
    {
      return Err(restart_topology_error(
        cargo_key,
        &format!(
          "replica {replica_key} has {} active {role} processes for {container_name:?}",
          active.len()
        ),
      ));
    }
  }

  let sandbox_required = needs_sandbox(cargo);
  let mut slots = Vec::new();
  for replica in &replicas {
    let replica_mappings = mappings
      .iter()
      .filter(|mapping| mapping.replica_key == replica.key)
      .collect::<Vec<_>>();
    let sandbox_mappings = replica_mappings
      .iter()
      .copied()
      .filter(|mapping| mapping.role == CargoReplicaProcessRole::Sandbox)
      .collect::<Vec<_>>();
    if sandbox_required {
      let [sandbox_mapping] = sandbox_mappings.as_slice() else {
        return Err(restart_topology_error(
          cargo_key,
          &format!(
            "replica {} has {} sandbox mappings instead of one",
            replica.key,
            sandbox_mappings.len()
          ),
        ));
      };
      slots.push(validate_restart_slot(ValidateRestartSlotOpts {
        cargo_key,
        replica,
        mapping: sandbox_mapping,
        expected_role: CargoReplicaProcessRole::Sandbox,
        expected_name: SANDBOX_LOGICAL_NAME,
        expected_essential: true,
        processes_by_key: &processes_by_key,
        stable_identities: &stable_identities,
      })?);
    } else {
      if !sandbox_mappings.is_empty() {
        return Err(restart_topology_error(
          cargo_key,
          &format!(
            "replica {} unexpectedly has a sandbox mapping",
            replica.key
          ),
        ));
      }
      if stable_identities.contains_key(&(
        replica.key,
        CargoReplicaProcessRole::Sandbox,
        SANDBOX_LOGICAL_NAME.to_owned(),
      )) {
        return Err(restart_topology_error(
          cargo_key,
          &format!(
            "replica {} unexpectedly has an active sandbox process",
            replica.key
          ),
        ));
      }
    }

    if let Some(mapping) = replica_mappings.iter().copied().find(|mapping| {
      mapping.role == CargoReplicaProcessRole::App
        && !desired_app_names.contains(mapping.container_name.as_str())
    }) {
      return Err(restart_topology_error(
        cargo_key,
        &format!(
          "replica {} has stale application mapping {:?}",
          replica.key, mapping.container_name
        ),
      ));
    }
    for declared in &cargo.spec.containers {
      let app_mappings = replica_mappings
        .iter()
        .copied()
        .filter(|mapping| {
          mapping.role == CargoReplicaProcessRole::App
            && mapping.container_name == declared.name
        })
        .collect::<Vec<_>>();
      let [mapping] = app_mappings.as_slice() else {
        return Err(restart_topology_error(
          cargo_key,
          &format!(
            "replica {} application {:?} has {} mappings instead of one",
            replica.key,
            declared.name,
            app_mappings.len()
          ),
        ));
      };
      slots.push(validate_restart_slot(ValidateRestartSlotOpts {
        cargo_key,
        replica,
        mapping,
        expected_role: CargoReplicaProcessRole::App,
        expected_name: &declared.name,
        expected_essential: declared.essential,
        processes_by_key: &processes_by_key,
        stable_identities: &stable_identities,
      })?);
    }
  }
  Ok(CargoRestartPlan { slots })
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::cargo_spec::CargoNetworkMode;

  use crate::utils::container::cargo_compiler::{
    LABEL_CONTAINER, LABEL_REPLICA_ORDINAL,
  };

  use super::super::{
    readiness::{
      requires_steady_state_readiness, restart_required_slots_ready,
    },
    tests::{declared_container, readiness_process},
  };
  use super::*;

  fn restart_replica(key: uuid::Uuid, ordinal: i32) -> CargoReplicaDb {
    let now = chrono::Utc::now().naive_utc();
    CargoReplicaDb {
      key,
      cargo_key: "global.api".to_owned(),
      ordinal,
      node_name: Some("node-a".to_owned()),
      created_at: now,
      updated_at: now,
    }
  }

  fn restart_process(
    replica: uuid::Uuid,
    ordinal: i32,
    process_key: &str,
    role: CargoReplicaProcessRole,
    logical_name: &str,
    essential: bool,
  ) -> Process {
    let process_name = match role {
      CargoReplicaProcessRole::Sandbox => {
        format!("global.api-r{ordinal}-sandbox.c")
      }
      CargoReplicaProcessRole::Init => {
        format!("init-global.api-r{ordinal}-{logical_name}.c")
      }
      CargoReplicaProcessRole::App => {
        format!("global.api-r{ordinal}-{logical_name}.c")
      }
    };
    let mut process = readiness_process(
      replica,
      &process_name,
      role.as_str(),
      logical_name,
      essential,
      role != CargoReplicaProcessRole::Init,
      None,
    );
    process.key = process_key.to_owned();
    process
      .data
      .config
      .as_mut()
      .unwrap()
      .labels
      .as_mut()
      .unwrap()
      .insert(LABEL_REPLICA_ORDINAL.to_owned(), ordinal.to_string());
    process
  }

  fn restart_mapping(
    replica_key: uuid::Uuid,
    process: &Process,
    role: CargoReplicaProcessRole,
    logical_name: &str,
    essential: bool,
  ) -> CargoReplicaProcessDb {
    let now = chrono::Utc::now().naive_utc();
    CargoReplicaProcessDb {
      key: uuid::Uuid::new_v4(),
      replica_key,
      process_key: Some(process.key.clone()),
      container_name: logical_name.to_owned(),
      role,
      essential,
      created_at: now,
      updated_at: now,
    }
  }

  fn restart_fixture(
    replicas: usize,
  ) -> (
    Cargo,
    Vec<CargoReplicaDb>,
    Vec<CargoReplicaProcessDb>,
    Vec<Process>,
  ) {
    restart_fixture_with_apps(
      replicas,
      &[("worker", true), ("api", true), ("metrics", false)],
    )
  }

  fn restart_fixture_with_apps(
    replicas: usize,
    applications: &[(&str, bool)],
  ) -> (
    Cargo,
    Vec<CargoReplicaDb>,
    Vec<CargoReplicaProcessDb>,
    Vec<Process>,
  ) {
    let mut cargo = Cargo::default();
    cargo.spec.cargo_key = "global.api".to_owned();
    cargo.spec.replicas = replicas;
    cargo.spec.init_containers = vec![declared_container("prepare", true)];
    cargo.spec.containers = applications
      .iter()
      .map(|(name, essential)| declared_container(name, *essential))
      .collect();
    let mut replica_rows = Vec::new();
    let mut mappings = Vec::new();
    let mut processes = Vec::new();
    for ordinal in 0..replicas as i32 {
      let replica_key = uuid::Uuid::new_v4();
      replica_rows.push(restart_replica(replica_key, ordinal));
      let mut expected = vec![(CargoReplicaProcessRole::Init, "prepare", true)];
      if needs_sandbox(&cargo) {
        expected.insert(
          0,
          (CargoReplicaProcessRole::Sandbox, SANDBOX_LOGICAL_NAME, true),
        );
      }
      expected.extend(applications.iter().map(|(name, essential)| {
        (CargoReplicaProcessRole::App, *name, *essential)
      }));
      for (role, name, essential) in expected {
        let process_key = format!("r{ordinal}-{role}-{name}");
        let process = restart_process(
          replica_key,
          ordinal,
          &process_key,
          role,
          name,
          essential,
        );
        mappings.push(restart_mapping(
          replica_key,
          &process,
          role,
          name,
          essential,
        ));
        processes.push(process);
      }
    }
    replica_rows.reverse();
    mappings.reverse();
    processes.reverse();
    (cargo, replica_rows, mappings, processes)
  }

  #[test]
  fn single_application_restart_accepts_direct_non_host_topologies() {
    for network_mode in [None, Some("private"), Some("none")] {
      let (mut cargo, replicas, mappings, processes) =
        restart_fixture_with_apps(1, &[("api", true)]);
      cargo.spec.network_mode =
        network_mode.map(|mode| CargoNetworkMode::new(mode).unwrap());

      let plan =
        plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
          .unwrap();
      assert_eq!(plan.slots.len(), 1);
      assert_eq!(plan.slots[0].role, CargoReplicaProcessRole::App);
      assert_eq!(plan.slots[0].container_name, "api");
      assert_eq!(plan.slots[0].process.key, "r0-app-api");
    }
  }

  #[test]
  fn single_application_restart_rejects_legacy_sandbox_topology() {
    let (cargo, replicas, mut mappings, mut processes) =
      restart_fixture_with_apps(1, &[("api", true)]);
    let replica = &replicas[0];
    let sandbox = restart_process(
      replica.key,
      replica.ordinal,
      "legacy-sandbox",
      CargoReplicaProcessRole::Sandbox,
      SANDBOX_LOGICAL_NAME,
      true,
    );
    mappings.push(restart_mapping(
      replica.key,
      &sandbox,
      CargoReplicaProcessRole::Sandbox,
      SANDBOX_LOGICAL_NAME,
      true,
    ));
    processes.push(sandbox);

    let error =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("unexpectedly has a sandbox mapping")
    );

    mappings.retain(|mapping| mapping.role != CargoReplicaProcessRole::Sandbox);
    let error =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("unexpectedly has an active sandbox process")
    );
  }

  #[test]
  fn cargo_restart_targets_only_apps_in_replica_and_declaration_order() {
    let (cargo, replicas, mappings, processes) = restart_fixture(2);
    let plan =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap();

    let restarted = plan
      .slots
      .iter()
      .filter(|slot| slot.role == CargoReplicaProcessRole::App)
      .map(|slot| slot.process.key.as_str())
      .collect::<Vec<_>>();
    assert_eq!(
      restarted,
      [
        "r0-app-worker",
        "r0-app-api",
        "r0-app-metrics",
        "r1-app-worker",
        "r1-app-api",
        "r1-app-metrics",
      ]
    );
    assert!(plan.slots.iter().all(|slot| {
      slot.role != CargoReplicaProcessRole::Init
        && !slot.process.key.contains("init")
    }));
    assert_eq!(
      plan
        .slots
        .iter()
        .filter(|slot| slot.role == CargoReplicaProcessRole::Sandbox)
        .count(),
      2
    );
  }

  #[test]
  fn cargo_restart_ignores_unmapped_rollout_artifacts() {
    let (cargo, replicas, mappings, mut processes) = restart_fixture(1);
    let mut candidate = processes
      .iter()
      .find(|process| process.key == "r0-app-api")
      .unwrap()
      .clone();
    candidate.key = "candidate-id".to_owned();
    candidate.name = "candidate-global.api-r0-api.c".to_owned();
    let mut retained = candidate.clone();
    retained.key = "retained-id".to_owned();
    retained.name = "tmp-global.api-r0-api.c".to_owned();
    processes.extend([candidate, retained]);

    let plan =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap();
    assert!(plan.slots.iter().all(|slot| {
      slot.process.key != "candidate-id" && slot.process.key != "retained-id"
    }));
  }

  #[test]
  fn cargo_restart_rejects_missing_sandbox_and_duplicate_active_app() {
    let (cargo, replicas, mut mappings, mut processes) = restart_fixture(1);
    let original_mappings = mappings.clone();
    let original_processes = processes.clone();
    mappings.retain(|mapping| mapping.role != CargoReplicaProcessRole::Sandbox);
    let error =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("0 sandbox mappings instead of one")
    );

    mappings = original_mappings;
    processes = original_processes;
    let mut duplicate = processes
      .iter()
      .find(|process| process.key == "r0-app-api")
      .unwrap()
      .clone();
    duplicate.key = "duplicate-api".to_owned();
    duplicate.name = "global.api-r0-api-duplicate.c".to_owned();
    processes.push(duplicate);
    let error =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap_err();
    assert!(error.to_string().contains("2 active app processes"));

    processes.retain(|process| process.key != "duplicate-api");
    let mut unexpected_sandbox = processes
      .iter()
      .find(|process| process.key == "r0-sandbox-_sandbox")
      .unwrap()
      .clone();
    unexpected_sandbox.key = "unexpected-sandbox".to_owned();
    unexpected_sandbox.name = "global.api-r0-other-sandbox.c".to_owned();
    unexpected_sandbox
      .data
      .config
      .as_mut()
      .unwrap()
      .labels
      .as_mut()
      .unwrap()
      .insert(LABEL_CONTAINER.to_owned(), "other".to_owned());
    processes.push(unexpected_sandbox);
    let error =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap_err();
    assert!(error.to_string().contains("unexpected active sandbox"));
  }

  #[test]
  fn host_cargo_restart_keeps_the_existing_no_sandbox_topology() {
    let (mut cargo, replicas, mut mappings, mut processes) = restart_fixture(1);
    cargo.spec.network_mode = Some(CargoNetworkMode::new("host").unwrap());
    mappings.retain(|mapping| mapping.role != CargoReplicaProcessRole::Sandbox);
    processes.retain(|process| {
      process_identity(process)
        .is_ok_and(|identity| identity.role != CargoReplicaProcessRole::Sandbox)
    });

    let plan =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap();
    assert!(
      plan
        .slots
        .iter()
        .all(|slot| slot.role == CargoReplicaProcessRole::App)
    );
  }

  #[test]
  fn multi_application_none_restart_preserves_the_required_sandbox() {
    let (mut cargo, replicas, mappings, processes) = restart_fixture(1);
    cargo.spec.network_mode = Some(CargoNetworkMode::new("none").unwrap());

    let plan =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap();
    assert_eq!(
      plan
        .slots
        .iter()
        .filter(|slot| slot.role == CargoReplicaProcessRole::Sandbox)
        .count(),
      1
    );
    assert_eq!(
      plan
        .slots
        .iter()
        .filter(|slot| slot.role == CargoReplicaProcessRole::App)
        .count(),
      cargo.spec.containers.len()
    );
  }

  #[test]
  fn application_network_escapes_do_not_change_restart_topology() {
    let (mut cargo, replicas, mappings, processes) = restart_fixture(1);
    for (container, network_mode) in
      cargo.spec.containers.iter_mut().zip(["host", "none"])
    {
      container
        .container_config
        .host_config
        .get_or_insert_default()
        .network_mode = Some(network_mode.to_owned());
    }

    let plan =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap();
    assert_eq!(
      plan
        .slots
        .iter()
        .filter(|slot| slot.role == CargoReplicaProcessRole::Sandbox)
        .count(),
      1
    );
    assert_eq!(
      plan
        .slots
        .iter()
        .filter(|slot| slot.role == CargoReplicaProcessRole::App)
        .map(|slot| slot.process.key.as_str())
        .collect::<Vec<_>>(),
      ["r0-app-worker", "r0-app-api", "r0-app-metrics"]
    );
  }

  #[test]
  fn cargo_restart_requires_every_mapped_app_but_readiness_ignores_nonessential()
   {
    let (cargo, replicas, mut mappings, processes) = restart_fixture(1);
    mappings.retain(|mapping| mapping.container_name != "metrics");
    let error =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("application \"metrics\" has 0 mappings")
    );

    assert!(!requires_steady_state_readiness(
      CargoReplicaProcessRole::App,
      false
    ));
  }

  #[test]
  fn cargo_restart_rechecks_the_complete_required_set_before_success() {
    let (cargo, replicas, mappings, processes) = restart_fixture(1);
    let mut plan =
      plan_cargo_restart(&cargo, &replicas, &mappings, &processes, "node-a")
        .unwrap();
    assert!(restart_required_slots_ready(&plan.slots));

    let metrics = plan
      .slots
      .iter()
      .find(|slot| slot.container_name == "metrics")
      .map(|slot| slot.process.key.clone())
      .unwrap();
    plan
      .slots
      .iter_mut()
      .find(|slot| slot.process.key == metrics)
      .unwrap()
      .process
      .data
      .state
      .as_mut()
      .unwrap()
      .running = Some(false);
    assert!(restart_required_slots_ready(&plan.slots));

    let worker = plan
      .slots
      .iter()
      .find(|slot| slot.container_name == "worker")
      .map(|slot| slot.process.key.clone())
      .unwrap();
    let set_running = |plan: &mut CargoRestartPlan, running| {
      plan
        .slots
        .iter_mut()
        .find(|slot| slot.process.key == worker)
        .unwrap()
        .process
        .data
        .state
        .as_mut()
        .unwrap()
        .running = Some(running);
    };
    set_running(&mut plan, false);
    assert!(!restart_required_slots_ready(&plan.slots));

    set_running(&mut plan, true);
    let sandbox = plan
      .slots
      .iter_mut()
      .find(|slot| slot.role == CargoReplicaProcessRole::Sandbox)
      .unwrap();
    sandbox.process.data.state.as_mut().unwrap().running = Some(false);
    assert!(!restart_required_slots_ready(&plan.slots));
  }
}
