use std::collections::{HashMap, HashSet};

use bollard_next::models::{ContainerInspectResponse, HealthConfig};
use nanocl_stubs::{cargo::Cargo, process::Process, system::NativeEventAction};

use crate::models::CargoReplicaProcessRole;

use super::{RuntimeSlot, is_rollout_process_name, needs_sandbox};
use crate::utils::container::cargo_compiler::{
  LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_REPLICA, LABEL_ROLE,
};

pub(super) fn healthcheck_enabled(healthcheck: Option<&HealthConfig>) -> bool {
  healthcheck.is_some_and(|healthcheck| {
    !matches!(
      healthcheck.test.as_deref(),
      Some([first, ..]) if first.eq_ignore_ascii_case("NONE")
    )
  })
}

pub(super) fn requires_steady_state_readiness(
  role: CargoReplicaProcessRole,
  essential: bool,
) -> bool {
  role == CargoReplicaProcessRole::Sandbox
    || (role == CargoReplicaProcessRole::App && essential)
}

pub(crate) fn promotion_actions(
  has_healthcheck: bool,
) -> Vec<NativeEventAction> {
  let mut actions = vec![NativeEventAction::Start];
  if has_healthcheck {
    actions.push(NativeEventAction::Healthy);
  }
  actions
}

pub(super) fn restart_required_slots_ready(slots: &[RuntimeSlot]) -> bool {
  slots
    .iter()
    .filter(|slot| requires_steady_state_readiness(slot.role, slot.essential))
    .all(|slot| {
      if slot.role == CargoReplicaProcessRole::Sandbox {
        observed_container_running(&slot.process.data)
      } else {
        observed_container_ready(&slot.process.data)
      }
    })
}

pub(super) fn required_candidate_topology_complete<'a>(
  cargo: &Cargo,
  slots: impl IntoIterator<Item = (CargoReplicaProcessRole, &'a str, bool)>,
) -> bool {
  let desired_apps = cargo
    .spec
    .containers
    .iter()
    .filter(|container| container.essential)
    .map(|container| container.name.as_str())
    .collect::<HashSet<_>>();
  let mut observed_apps = HashSet::new();
  let mut duplicate_app = false;
  let mut sandboxes = 0usize;
  for (role, name, essential) in slots {
    match role {
      CargoReplicaProcessRole::Sandbox => sandboxes += 1,
      CargoReplicaProcessRole::App
        if essential && !observed_apps.insert(name) =>
      {
        duplicate_app = true;
      }
      _ => {}
    }
  }
  let sandbox_complete = if needs_sandbox(cargo) {
    sandboxes == 1
  } else {
    sandboxes == 0
  };
  sandbox_complete && !duplicate_app && observed_apps == desired_apps
}

/// Whether any current declared or observed application process uses an
/// enabled Docker healthcheck. Sandboxes, init containers, and retained old
/// rollout processes do not participate.
pub fn has_container_healthcheck(cargo: &Cargo, instances: &[Process]) -> bool {
  cargo.spec.containers.iter().any(|container| {
    container.essential
      && healthcheck_enabled(container.container_config.healthcheck.as_ref())
  }) || instances.iter().any(|instance| {
    if is_rollout_process_name(&instance.name) {
      return false;
    }
    let labels = instance
      .data
      .config
      .as_ref()
      .and_then(|config| config.labels.as_ref());
    if labels
      .and_then(|labels| labels.get(LABEL_ROLE))
      .map(String::as_str)
      != Some("app")
      || labels
        .and_then(|labels| labels.get(LABEL_ESSENTIAL))
        .map(String::as_str)
        != Some("true")
    {
      return false;
    }
    let config_healthcheck = instance
      .data
      .config
      .as_ref()
      .and_then(|config| config.healthcheck.as_ref());
    healthcheck_enabled(config_healthcheck)
      || instance
        .data
        .state
        .as_ref()
        .and_then(|container_state| container_state.health.as_ref())
        .is_some()
  })
}

pub(super) fn observed_container_running(
  observed: &ContainerInspectResponse,
) -> bool {
  observed.state.as_ref().and_then(|state| state.running) == Some(true)
}

pub(super) fn observed_container_ready(
  observed: &ContainerInspectResponse,
) -> bool {
  if !observed_container_running(observed) {
    return false;
  }
  let healthcheck = observed
    .config
    .as_ref()
    .and_then(|config| config.healthcheck.as_ref());
  if !healthcheck_enabled(healthcheck) {
    return true;
  }
  observed
    .state
    .as_ref()
    .and_then(|state| state.health.as_ref())
    .and_then(|health| health.status.as_ref())
    .is_some_and(|status| status.to_string() == "healthy")
}

fn observed_process_ready(process: &Process) -> bool {
  observed_container_ready(&process.data)
}

#[derive(Default)]
struct ReplicaReadinessObservation {
  sandboxes: Vec<bool>,
  essential_apps: HashMap<String, bool>,
  invalid_required_app: bool,
}

/// Reduce fresh persisted observations without selecting a first container.
/// This legacy aggregate bridge is deliberately strict: every desired replica
/// must have its complete essential application set and exactly the sandbox
/// cardinality selected by the desired Cargo topology.
pub(crate) fn all_desired_replicas_ready(
  cargo: &Cargo,
  desired_replica_keys: &HashSet<uuid::Uuid>,
  instances: &[Process],
) -> bool {
  let desired_apps = cargo
    .spec
    .containers
    .iter()
    .filter(|container| container.essential)
    .map(|container| container.name.as_str())
    .collect::<HashSet<_>>();
  let mut replicas = HashMap::<uuid::Uuid, ReplicaReadinessObservation>::new();
  for process in instances {
    if is_rollout_process_name(&process.name) {
      continue;
    }
    let Some(labels) = process
      .data
      .config
      .as_ref()
      .and_then(|config| config.labels.as_ref())
    else {
      continue;
    };
    let Some(replica_key) = labels
      .get(LABEL_REPLICA)
      .and_then(|key| key.parse::<uuid::Uuid>().ok())
    else {
      continue;
    };
    let observation = replicas.entry(replica_key).or_default();
    match labels.get(LABEL_ROLE).map(String::as_str) {
      Some("sandbox") => {
        observation
          .sandboxes
          .push(observed_container_running(&process.data));
      }
      Some("app") => {
        let essential = match labels.get(LABEL_ESSENTIAL).map(String::as_str) {
          Some("true") => true,
          Some("false") => false,
          _ => {
            observation.invalid_required_app = true;
            continue;
          }
        };
        if !essential {
          continue;
        }
        let Some(name) = labels.get(LABEL_CONTAINER) else {
          observation.invalid_required_app = true;
          continue;
        };
        if observation
          .essential_apps
          .insert(name.clone(), observed_process_ready(process))
          .is_some()
        {
          observation.invalid_required_app = true;
        }
      }
      _ => {}
    }
  }
  if desired_replica_keys.len() != cargo.spec.replicas
    || replicas.keys().copied().collect::<HashSet<_>>() != *desired_replica_keys
  {
    return false;
  }
  let sandbox_required = needs_sandbox(cargo);
  replicas.values().all(|observation| {
    !observation.invalid_required_app
      && observation.essential_apps.len() == desired_apps.len()
      && desired_apps
        .iter()
        .all(|name| observation.essential_apps.get(*name) == Some(&true))
      && if sandbox_required {
        observation.sandboxes == [true]
      } else {
        observation.sandboxes.is_empty()
      }
  })
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use bollard_next::models::{Health, HealthConfig, HealthStatusEnum};
  use nanocl_stubs::{cargo_spec::CargoNetworkMode, system::NativeEventAction};

  use super::super::{
    SANDBOX_LOGICAL_NAME,
    tests::{
      declared_container, readiness_cargo, readiness_process,
      ready_replica_processes, topology_cargo,
    },
  };
  use super::*;

  #[test]
  fn disabled_healthcheck_does_not_gate_readiness() {
    let disabled = HealthConfig {
      test: Some(vec!["NONE".to_owned()]),
      ..Default::default()
    };
    let enabled = HealthConfig {
      test: Some(vec!["CMD".to_owned(), "true".to_owned()]),
      ..Default::default()
    };
    assert!(!healthcheck_enabled(Some(&disabled)));
    assert!(healthcheck_enabled(Some(&enabled)));
  }

  #[test]
  fn steady_state_readiness_excludes_completed_init_and_nonessential_apps() {
    assert!(requires_steady_state_readiness(
      CargoReplicaProcessRole::Sandbox,
      true
    ));
    assert!(requires_steady_state_readiness(
      CargoReplicaProcessRole::App,
      true
    ));
    assert!(!requires_steady_state_readiness(
      CargoReplicaProcessRole::App,
      false
    ));
    assert!(!requires_steady_state_readiness(
      CargoReplicaProcessRole::Init,
      true
    ));
  }

  #[test]
  fn successful_promotion_keeps_the_nightly_event_order() {
    assert_eq!(promotion_actions(false), vec![NativeEventAction::Start]);
    assert_eq!(
      promotion_actions(true),
      vec![NativeEventAction::Start, NativeEventAction::Healthy]
    );
  }

  #[test]
  fn candidate_topology_requires_sandbox_and_every_essential_application() {
    let mut cargo = readiness_cargo(1);
    cargo
      .spec
      .containers
      .insert(1, declared_container("worker", true));
    let complete = [
      (CargoReplicaProcessRole::Sandbox, SANDBOX_LOGICAL_NAME, true),
      (CargoReplicaProcessRole::Init, "migrate", true),
      (CargoReplicaProcessRole::App, "api", true),
      (CargoReplicaProcessRole::App, "worker", true),
      (CargoReplicaProcessRole::App, "metrics", false),
    ];
    assert!(required_candidate_topology_complete(&cargo, complete));
    assert!(!required_candidate_topology_complete(
      &cargo,
      complete
        .into_iter()
        .filter(|(_, name, _)| *name != "worker")
    ));
    assert!(!required_candidate_topology_complete(
      &cargo,
      complete
        .into_iter()
        .filter(|(role, _, _)| { *role != CargoReplicaProcessRole::Sandbox })
    ));

    cargo.spec.network_mode = Some(CargoNetworkMode::new("host").unwrap());
    assert!(required_candidate_topology_complete(
      &cargo,
      complete
        .into_iter()
        .filter(|(role, _, _)| { *role != CargoReplicaProcessRole::Sandbox })
    ));
  }

  #[test]
  fn candidate_topology_enforces_direct_and_sandbox_cardinality() {
    let direct = topology_cargo(1, 1, None);
    let direct_app = [
      (CargoReplicaProcessRole::Init, "init-0", true),
      (CargoReplicaProcessRole::App, "app-0", true),
    ];
    assert!(required_candidate_topology_complete(&direct, direct_app));
    assert!(!required_candidate_topology_complete(
      &direct,
      direct_app.into_iter().chain([(
        CargoReplicaProcessRole::Sandbox,
        SANDBOX_LOGICAL_NAME,
        true,
      )])
    ));

    let shared = topology_cargo(2, 1, None);
    let shared_apps = [
      (CargoReplicaProcessRole::Init, "init-0", true),
      (CargoReplicaProcessRole::App, "app-0", true),
      (CargoReplicaProcessRole::App, "app-1", true),
    ];
    assert!(!required_candidate_topology_complete(&shared, shared_apps));
    let complete = [
      (CargoReplicaProcessRole::Init, "init-0", true),
      (CargoReplicaProcessRole::App, "app-0", true),
      (CargoReplicaProcessRole::App, "app-1", true),
      (CargoReplicaProcessRole::Sandbox, SANDBOX_LOGICAL_NAME, true),
    ];
    assert!(required_candidate_topology_complete(&shared, complete));
    assert!(!required_candidate_topology_complete(
      &shared,
      complete.into_iter().chain([(
        CargoReplicaProcessRole::Sandbox,
        SANDBOX_LOGICAL_NAME,
        true,
      )])
    ));
  }

  #[test]
  fn aggregate_readiness_enforces_direct_and_sandbox_cardinality() {
    let replica = uuid::Uuid::new_v4();
    let desired = HashSet::from([replica]);
    let direct = topology_cargo(1, 0, None);
    let direct_app = readiness_process(
      replica,
      "global.api-app.c",
      "app",
      "app-0",
      true,
      true,
      None,
    );
    assert!(all_desired_replicas_ready(
      &direct,
      &desired,
      std::slice::from_ref(&direct_app)
    ));
    let unexpected_sandbox = readiness_process(
      replica,
      "global.api-sandbox.c",
      "sandbox",
      SANDBOX_LOGICAL_NAME,
      true,
      true,
      None,
    );
    assert!(!all_desired_replicas_ready(
      &direct,
      &desired,
      &[direct_app, unexpected_sandbox.clone()]
    ));

    let shared = topology_cargo(2, 0, None);
    let apps = [
      readiness_process(
        replica,
        "global.api-app-0.c",
        "app",
        "app-0",
        true,
        true,
        None,
      ),
      readiness_process(
        replica,
        "global.api-app-1.c",
        "app",
        "app-1",
        true,
        true,
        None,
      ),
    ];
    assert!(!all_desired_replicas_ready(&shared, &desired, &apps));
    let mut complete = apps.to_vec();
    complete.push(unexpected_sandbox.clone());
    assert!(all_desired_replicas_ready(&shared, &desired, &complete));
    complete.push(unexpected_sandbox);
    assert!(!all_desired_replicas_ready(&shared, &desired, &complete));
  }

  #[test]
  fn complete_desired_replica_is_ready_with_healthless_or_disabled_app() {
    let replica = uuid::Uuid::new_v4();
    let cargo = readiness_cargo(1);
    let desired = HashSet::from([replica]);
    let mut processes = ready_replica_processes(replica);
    assert!(all_desired_replicas_ready(&cargo, &desired, &processes));

    processes[1].data.config.as_mut().unwrap().healthcheck =
      Some(HealthConfig {
        test: Some(vec!["NONE".to_owned()]),
        ..Default::default()
      });
    assert!(all_desired_replicas_ready(&cargo, &desired, &processes));

    processes[1].data.config.as_mut().unwrap().healthcheck =
      Some(HealthConfig {
        test: Some(vec!["CMD".to_owned(), "true".to_owned()]),
        ..Default::default()
      });
    processes[1].data.state.as_mut().unwrap().health = Some(Health {
      status: Some(HealthStatusEnum::HEALTHY),
      ..Default::default()
    });
    assert!(all_desired_replicas_ready(&cargo, &desired, &processes));
  }

  #[test]
  fn complete_replica_readiness_uses_only_required_steady_state_processes() {
    let replica = uuid::Uuid::new_v4();
    let mut cargo = readiness_cargo(1);
    cargo
      .spec
      .containers
      .insert(1, declared_container("worker", true));
    let desired = HashSet::from([replica]);
    let mut processes = vec![
      readiness_process(
        replica,
        "global.api-sandbox.c",
        "sandbox",
        SANDBOX_LOGICAL_NAME,
        true,
        true,
        None,
      ),
      readiness_process(
        replica,
        "global.api-api.c",
        "app",
        "api",
        true,
        true,
        Some(HealthStatusEnum::HEALTHY),
      ),
      readiness_process(
        replica,
        "global.api-worker.c",
        "app",
        "worker",
        true,
        true,
        Some(HealthStatusEnum::STARTING),
      ),
      readiness_process(
        replica,
        "global.api-metrics.c",
        "app",
        "metrics",
        false,
        true,
        Some(HealthStatusEnum::UNHEALTHY),
      ),
      readiness_process(
        replica,
        "global.api-migrate.init.c",
        "init",
        "migrate",
        true,
        false,
        None,
      ),
    ];

    // One healthy application cannot promote a two-application candidate.
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));
    processes[2].data.state.as_mut().unwrap().health = Some(Health {
      status: Some(HealthStatusEnum::HEALTHY),
      ..Default::default()
    });
    assert!(all_desired_replicas_ready(&cargo, &desired, &processes));

    // A running healthless essential application is ready alongside a
    // healthchecked essential application. Completed init and an unhealthy
    // nonessential application do not participate in steady-state readiness.
    processes[2].data.config.as_mut().unwrap().healthcheck = None;
    processes[2].data.state.as_mut().unwrap().health = None;
    assert!(all_desired_replicas_ready(&cargo, &desired, &processes));

    let worker = processes.remove(2);
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));
    processes.insert(2, worker);
    processes[0].data.state.as_mut().unwrap().running = Some(false);
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));
    processes.remove(0);
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));
  }

  #[test]
  fn readiness_requires_exact_durable_replica_and_essential_app_sets() {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let cargo = readiness_cargo(2);
    let desired = HashSet::from([first, second]);
    let mut processes = ready_replica_processes(first);
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));

    processes.extend(ready_replica_processes(second));
    assert!(all_desired_replicas_ready(&cargo, &desired, &processes));

    let orphan = uuid::Uuid::new_v4();
    processes.extend(ready_replica_processes(orphan));
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));

    processes.retain(|process| {
      process
        .data
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref())
        .and_then(|labels| labels.get(LABEL_REPLICA))
        != Some(&orphan.to_string())
    });
    processes.retain(|process| {
      let labels = process
        .data
        .config
        .as_ref()
        .unwrap()
        .labels
        .as_ref()
        .unwrap();
      labels.get(LABEL_REPLICA) != Some(&second.to_string())
        || labels.get(LABEL_CONTAINER).map(String::as_str) != Some("api")
    });
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));
  }

  #[test]
  fn readiness_rejects_unhealthy_missing_and_duplicate_runtime_identity() {
    let replica = uuid::Uuid::new_v4();
    let cargo = readiness_cargo(1);
    let desired = HashSet::from([replica]);
    let mut processes = ready_replica_processes(replica);

    let mut unhealthy = readiness_process(
      replica,
      "global.api-api.c",
      "app",
      "api",
      true,
      true,
      Some(HealthStatusEnum::UNHEALTHY),
    );
    processes[1] = unhealthy.clone();
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));

    unhealthy.data.state.as_mut().unwrap().health = Some(Health {
      status: Some(HealthStatusEnum::HEALTHY),
      ..Default::default()
    });
    processes[1] = unhealthy.clone();
    processes.push(unhealthy);
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));

    processes = ready_replica_processes(replica);
    processes.push(processes[0].clone());
    assert!(!all_desired_replicas_ready(&cargo, &desired, &processes));

    processes = ready_replica_processes(replica);
    let sidecar = readiness_process(
      replica,
      "global.api-metrics.c",
      "app",
      "metrics",
      false,
      true,
      None,
    );
    processes.push(sidecar.clone());
    processes.push(sidecar);
    assert!(all_desired_replicas_ready(&cargo, &desired, &processes));
  }

  #[test]
  fn host_mode_needs_no_sandbox_and_pending_generations_are_ignored() {
    let replica = uuid::Uuid::new_v4();
    let mut cargo = readiness_cargo(1);
    let desired = HashSet::from([replica]);
    let app = readiness_process(
      replica,
      "global.api-api.c",
      "app",
      "api",
      true,
      true,
      None,
    );
    assert!(!all_desired_replicas_ready(
      &cargo,
      &desired,
      std::slice::from_ref(&app)
    ));

    cargo.spec.network_mode = Some(CargoNetworkMode::new("host").unwrap());
    assert!(all_desired_replicas_ready(
      &cargo,
      &desired,
      std::slice::from_ref(&app)
    ));

    let mut with_sandbox = vec![app.clone()];
    with_sandbox.push(ready_replica_processes(replica).remove(0));
    assert!(!all_desired_replicas_ready(&cargo, &desired, &with_sandbox));

    let mut retained = app.clone();
    retained.name = "global.api-api.tmp.c".to_owned();
    let mut candidate = app;
    candidate.name = "global.api-api.candidate.c".to_owned();
    assert!(!all_desired_replicas_ready(
      &cargo,
      &desired,
      &[retained, candidate]
    ));
  }
}
