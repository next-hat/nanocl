use nanocl_error::io::{IoError, IoResult};
use nanocld_client::stubs::cargo_spec::{
  CargoSpec, CargoSpecPatch, CargoSpecRevision, ContainerSpec,
};

use crate::models::CargoPatchOpts;

/// Build a complete application-container replacement for a Cargo patch.
pub(crate) fn build_cargo_patch(
  opts: &CargoPatchOpts,
  current: &[ContainerSpec],
) -> IoResult<CargoSpecPatch> {
  let target_name = match opts.container.as_ref() {
    Some(name) => name.clone(),
    None => match current {
      [] => {
        return Err(IoError::invalid_input(
          "Cargo patch",
          "the Cargo has no application container to update",
        ));
      }
      [container] => container.name.clone(),
      _ => {
        return Err(IoError::invalid_input(
          "Cargo patch",
          "--container is required when the Cargo has multiple application containers",
        ));
      }
    },
  };

  let mut containers = current.to_vec();
  let mut matches = 0;
  for container in &mut containers {
    if container.name != target_name {
      continue;
    }
    matches += 1;
    if let Some(image) = &opts.image {
      container.container_config.image = Some(image.clone());
    }
    if let Some(env) = &opts.env {
      container.container_config.env = Some(env.clone());
    }
    if let Some(volumes) = &opts.volumes {
      container
        .container_config
        .host_config
        .get_or_insert_default()
        .binds = Some(volumes.clone());
    }
  }

  match matches {
    0 => {
      return Err(IoError::invalid_input(
        "Cargo patch".to_owned(),
        format!("application container {target_name:?} does not exist"),
      ));
    }
    1 => {}
    _ => {
      return Err(IoError::invalid_data(
        "Cargo patch".to_owned(),
        format!("application container {target_name:?} is duplicated"),
      ));
    }
  }

  Ok(CargoSpecPatch {
    containers: Some(containers),
    ..Default::default()
  })
}

/// Convert a stored Cargo revision back to its complete desired declaration.
pub(crate) fn cargo_spec_from_revision(
  revision: &CargoSpecRevision,
) -> CargoSpec {
  CargoSpec {
    name: revision.name.clone(),
    metadata: revision.metadata.clone(),
    replicas: revision.replicas,
    network_mode: revision.network_mode.clone(),
    port_bindings: revision.port_bindings.clone(),
    hostname: revision.hostname.clone(),
    dns: revision.dns.clone(),
    secrets: revision.secrets.clone(),
    placement: revision.placement.clone(),
    resource_requirement: revision.resource_requirement.clone(),
    init_containers: revision.init_containers.clone(),
    containers: revision.containers.clone(),
  }
}
