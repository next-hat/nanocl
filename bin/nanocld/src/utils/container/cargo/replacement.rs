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

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use bollard_next::models::PortBinding;
  use nanocl_stubs::{cargo::Cargo, cargo_spec::CargoNetworkMode};

  use super::super::tests::readiness_cargo;
  use super::*;

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
