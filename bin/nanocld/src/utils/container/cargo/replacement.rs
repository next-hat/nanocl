use bollard_next::models::ContainerInspectResponse;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::cargo::Cargo;

use super::cargo_network_mode;

fn has_fixed_port_bindings(cargo: &Cargo) -> bool {
  cargo.spec.port_bindings.as_ref().is_some_and(|bindings| {
    bindings.values().any(|bindings| {
      bindings.as_ref().is_some_and(|bindings| {
        bindings.iter().any(|binding| {
          binding.host_port.as_deref().is_some_and(|port| {
            let port = port.trim();
            !port.is_empty() && port != "0"
          })
        })
      })
    })
  })
}

pub(super) fn requires_removal_before_replacement(cargo: &Cargo) -> bool {
  has_fixed_port_bindings(cargo)
}

pub(super) fn validate_local_fixed_port_ownership(
  cargo: &Cargo,
  local_replicas: usize,
) -> IoResult<()> {
  if local_replicas > 1 && has_fixed_port_bindings(cargo) {
    return Err(IoError::other(
      "CargoPortOwnership",
      &format!(
        "Cargo {:?} assigns {local_replicas} replicas with a fixed host port to one node",
        cargo.spec.cargo_key
      ),
    ));
  }
  Ok(())
}

fn uses_host_network(cargo: &Cargo) -> bool {
  cargo_network_mode(cargo) == "host"
    || cargo
      .spec
      .containers
      .iter()
      .chain(&cargo.spec.init_containers)
      .any(|container| {
        container
          .container_config
          .host_config
          .as_ref()
          .and_then(|host| host.network_mode.as_deref())
          == Some("host")
      })
}

pub(super) fn requires_stop_before_replacement(cargo: &Cargo) -> bool {
  requires_removal_before_replacement(cargo) || uses_host_network(cargo)
}

pub(super) fn confirm_init_stopped(
  inspected: &ContainerInspectResponse,
) -> IoResult<()> {
  if inspected.state.as_ref().and_then(|state| state.running) != Some(false) {
    return Err(IoError::interrupted(
      "StopCargoProcess",
      "Docker has not confirmed the old init container stopped",
    ));
  }
  Ok(())
}

#[cfg(test)]
pub(super) mod stop_fault {
  use std::{
    collections::HashSet,
    sync::{LazyLock, Mutex},
  };

  use bollard_next::models::ContainerInspectResponse;

  static FAULTS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

  pub(crate) struct InitStopFaultGuard(String);

  impl Drop for InitStopFaultGuard {
    fn drop(&mut self) {
      FAULTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&self.0);
    }
  }

  pub(crate) fn forget_next_init_stop(key: &str) -> InitStopFaultGuard {
    assert!(FAULTS.lock().unwrap().insert(key.to_owned()));
    InitStopFaultGuard(key.to_owned())
  }

  pub(crate) fn observe(
    key: &str,
    mut inspected: ContainerInspectResponse,
  ) -> ContainerInspectResponse {
    if FAULTS.lock().unwrap().remove(key) {
      inspected.state.get_or_insert_with(Default::default).running = None;
    }
    inspected
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use bollard_next::models::{ContainerState, HostConfig, PortBinding};
  use nanocl_stubs::{cargo::Cargo, cargo_spec::CargoNetworkMode};

  use super::super::tests::{readiness_cargo, topology_cargo};
  use super::*;

  #[test]
  fn replacement_init_stop_requires_explicit_stopped_evidence() {
    let mut inspected = ContainerInspectResponse::default();
    let error = confirm_init_stopped(&inspected).unwrap_err();
    assert_eq!(error.context(), Some("StopCargoProcess"));
    assert_eq!(error.inner.kind(), std::io::ErrorKind::Interrupted);
    for running in [None, Some(true), Some(false)] {
      inspected.state = Some(ContainerState {
        running,
        ..Default::default()
      });
      assert_eq!(
        confirm_init_stopped(&inspected).is_ok(),
        running == Some(false)
      );
    }
  }

  #[test]
  fn init_stop_fault_is_scoped_one_shot_and_cleared_on_drop() {
    let key = format!("global.init-stop-fault-{}", uuid::Uuid::new_v4());
    let stopped = ContainerInspectResponse {
      state: Some(ContainerState {
        running: Some(false),
        ..Default::default()
      }),
      ..Default::default()
    };
    let guard = stop_fault::forget_next_init_stop(&key);
    assert!(
      confirm_init_stopped(&stop_fault::observe(
        "global.other",
        stopped.clone()
      ))
      .is_ok()
    );
    assert!(
      confirm_init_stopped(&stop_fault::observe(&key, stopped.clone()))
        .is_err()
    );
    assert!(
      confirm_init_stopped(&stop_fault::observe(&key, stopped.clone())).is_ok()
    );
    drop(guard);
    let guard = stop_fault::forget_next_init_stop(&key);
    drop(guard);
    assert!(confirm_init_stopped(&stop_fault::observe(&key, stopped)).is_ok());
  }

  #[test]
  fn init_host_network_requires_stop_without_removal() {
    let mut cargo = topology_cargo(1, 1, None);
    cargo.spec.init_containers[0].container_config.host_config =
      Some(HostConfig {
        network_mode: Some("host".to_owned()),
        ..Default::default()
      });
    assert!(requires_stop_before_replacement(&cargo));
    assert!(!requires_removal_before_replacement(&cargo));
    cargo.spec.init_containers[0]
      .container_config
      .host_config
      .as_mut()
      .unwrap()
      .network_mode = None;
    assert!(!requires_stop_before_replacement(&cargo));
    assert!(!requires_removal_before_replacement(&cargo));
  }

  #[test]
  fn cargo_port_bindings_require_stop_before_replacement() {
    let mut cargo = Cargo::default();
    cargo.spec.port_bindings = Some(HashMap::from([(
      "8080/tcp".to_owned(),
      Some(vec![PortBinding {
        host_ip: Some("0.0.0.0".to_owned()),
        host_port: Some("8080".to_owned()),
      }]),
    )]));
    assert!(requires_stop_before_replacement(&cargo));
    assert!(requires_removal_before_replacement(&cargo));

    cargo
      .spec
      .port_bindings
      .as_mut()
      .unwrap()
      .values_mut()
      .next()
      .unwrap()
      .as_mut()
      .unwrap()[0]
      .host_port = Some("0".to_owned());
    assert!(!requires_stop_before_replacement(&cargo));
    assert!(!requires_removal_before_replacement(&cargo));
  }

  #[test]
  fn multiple_local_replicas_cannot_share_a_fixed_host_port() {
    let mut cargo = readiness_cargo(2);
    cargo.spec.port_bindings = Some(HashMap::from([(
      "8080/tcp".to_owned(),
      Some(vec![PortBinding {
        host_ip: Some("0.0.0.0".to_owned()),
        host_port: Some("8080".to_owned()),
      }]),
    )]));
    let error = validate_local_fixed_port_ownership(&cargo, 2).unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains("fixed host port"));

    cargo
      .spec
      .port_bindings
      .as_mut()
      .unwrap()
      .values_mut()
      .next()
      .unwrap()
      .as_mut()
      .unwrap()[0]
      .host_port = Some("0".to_owned());
    assert!(validate_local_fixed_port_ownership(&cargo, 2).is_ok());
  }

  #[test]
  fn host_network_requires_stop_but_non_host_network_does_not() {
    let mut cargo = Cargo::default();
    cargo.spec.network_mode = Some(CargoNetworkMode::new("host").unwrap());
    assert!(requires_stop_before_replacement(&cargo));
    assert!(!requires_removal_before_replacement(&cargo));
    cargo.spec.network_mode = Some(CargoNetworkMode::new("private").unwrap());
    assert!(!requires_stop_before_replacement(&cargo));
  }
}
