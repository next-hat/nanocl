/// Handle object creation, deletion, update, read and inspect
/// For a cargo object in the database.
/// An object will emit an event when it is created, updated or deleted.
///
use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoError,
};
use nanocl_stubs::{
  cargo::{Cargo, CargoDeleteQuery, CargoInspect},
  cargo_spec::{CargoSpec, CargoSpecPatch, CargoSpecValidationError},
  resource_key::ResourceKey,
  system::{
    NativeEventAction, ObjPsHealthStatusKind, ObjPsStatusKind,
    ObjPsStatusPartial,
  },
};

use crate::{
  models::{
    CargoDb, CargoObjCreateIn, CargoObjPatchIn, CargoObjPutIn, CargoReplicaDb,
    ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb, SpecDb, SystemState,
  },
  repositories::generic::*,
  utils,
};

use super::generic::*;

fn validate_cargo_spec(spec: &CargoSpec) -> HttpResult<()> {
  spec.validate().map_err(|error| {
    IoError::invalid_input("CargoSpec".to_owned(), error.to_string()).into()
  })
}

fn validate_supported_rollout_change(
  current: &CargoSpec,
  next: &CargoSpec,
) -> Result<(), IoError> {
  if current.replicas != next.replicas {
    return Err(IoError::invalid_input(
      "CargoUpdate".to_owned(),
      "Changing CargoSpec.Replicas requires replica scale reconciliation and is not supported by the current safe rollout"
        .to_owned(),
    ));
  }
  if current.placement != next.placement {
    return Err(IoError::invalid_input(
      "CargoUpdate".to_owned(),
      "Changing CargoSpec.Placement requires replica relocation and is not supported by the current safe rollout"
        .to_owned(),
    ));
  }
  Ok(())
}

/// Apply a Cargo patch using whole-field replacement semantics.
fn apply_cargo_spec_patch(
  mut current: CargoSpec,
  patch: &CargoSpecPatch,
) -> Result<CargoSpec, CargoSpecValidationError> {
  if let Some(name) = &patch.name {
    current.name.clone_from(name);
  }
  if let Some(metadata) = &patch.metadata {
    current.metadata = Some(metadata.clone());
  }
  if let Some(replicas) = patch.replicas {
    current.replicas = replicas;
  }
  if let Some(network_mode) = &patch.network_mode {
    current.network_mode = Some(network_mode.clone());
  }
  if let Some(port_bindings) = &patch.port_bindings {
    current.port_bindings = Some(port_bindings.clone());
  }
  if let Some(hostname) = &patch.hostname {
    current.hostname = Some(hostname.clone());
  }
  if let Some(dns) = &patch.dns {
    current.dns = Some(dns.clone());
  }
  if let Some(secrets) = &patch.secrets {
    current.secrets.clone_from(secrets);
  }
  if let Some(placement) = &patch.placement {
    current.placement = Some(placement.clone());
  }
  if let Some(resource_requirement) = &patch.resource_requirement {
    current.resource_requirement = Some(resource_requirement.clone());
  }
  if let Some(init_containers) = &patch.init_containers {
    current.init_containers.clone_from(init_containers);
  }
  if let Some(containers) = &patch.containers {
    current.containers.clone_from(containers);
  }
  current.validate()?;
  Ok(current)
}

impl ObjCreate for CargoDb {
  type ObjCreateIn = CargoObjCreateIn;
  type ObjCreateOut = Cargo;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    validate_cargo_spec(&obj.spec)?;
    // validate the name of a cargo to include on a-z, A-Z, 0-9, and -_
    if !obj
      .spec
      .name
      .chars()
      .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
      return Err(HttpError::bad_request(
        "Cargo name can only contain a-z, A-Z, 0-9, and -_",
      ));
    }
    let key = ResourceKey::new(&obj.spec.name, &obj.namespace)
      .map_err(HttpError::bad_request)?
      .to_string();
    let new_spec = SpecDb::try_from_cargo_spec(&key, &obj.version, &obj.spec)?;
    let spec = SpecDb::create_from(new_spec, &state.inner.pool)
      .await?
      .try_to_cargo_spec_revision()?;
    let status = ObjPsStatusPartial {
      key: key.clone(),
      wanted: ObjPsStatusKind::Create,
      prev_wanted: ObjPsStatusKind::Create,
      actual: ObjPsStatusKind::Create,
      prev_actual: ObjPsStatusKind::Create,
      health: ObjPsHealthStatusKind::Unknown,
    };
    let status = ObjPsStatusDb::create_from(status, &state.inner.pool).await?;
    let new_item = CargoDb {
      key: key.clone(),
      name: obj.spec.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: obj.namespace.clone(),
      status_key: key,
      spec_key: spec.key,
    };
    let cargo = CargoDb::create_from(new_item, &state.inner.pool)
      .await?
      .with_spec(&(
        spec,
        status
          .try_into()
          .map_err(HttpError::internal_server_error)?,
      ));
    Ok(cargo)
  }
}

impl ObjDelByPk for CargoDb {
  type ObjDelOut = Cargo;
  type ObjDelOpts = CargoDeleteQuery;

  fn get_del_event() -> NativeEventAction {
    NativeEventAction::Destroying
  }

  async fn fn_del_obj_by_pk(
    pk: &str,
    opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let cargo = CargoDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let processes = ProcessDb::read_by_kind_key(
      &cargo.spec.cargo_key,
      None,
      &state.inner.pool,
    )
    .await?;
    let (_, _, _, running) =
      utils::container::generic::count_status(&processes);
    if running > 0 && !opts.force.unwrap_or(false) {
      return Err(HttpError::bad_request(
        "Unable to delete cargo with running instances without force option",
      ));
    }
    let status = ObjPsStatusDb::read_by_pk(pk, &state.inner.pool).await?;
    let new_status = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Destroy.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(ObjPsStatusKind::Destroying.to_string()),
      prev_actual: Some(status.actual),
      ..Default::default()
    };
    ObjPsStatusDb::update_pk(pk, new_status, &state.inner.pool).await?;
    Ok(cargo)
  }
}

impl ObjPutByPk for CargoDb {
  type ObjPutIn = CargoObjPutIn;
  type ObjPutOut = Cargo;

  async fn fn_put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut> {
    validate_cargo_spec(&obj.spec)?;
    let cargo = CargoDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let current = SpecDb::read_by_pk(&cargo.spec.key, &state.inner.pool)
      .await?
      .try_to_cargo_spec()?;
    validate_supported_rollout_change(&current, &obj.spec)
      .map_err(HttpError::from)?;
    let replicas = CargoReplicaDb::list_by_cargo(pk, &state.inner.pool).await?;
    if !replicas.is_empty()
      && (replicas.len() != current.replicas
        || replicas.iter().any(|replica| replica.node_name.is_none()))
    {
      return Err(IoError::invalid_input(
        "CargoUpdate".to_owned(),
        "Cargo update requires every durable replica to have a committed node assignment"
          .to_owned(),
      )
      .into());
    }
    if replicas.iter().any(|replica| {
      replica.node_name.as_deref().is_some_and(|node| {
        node != state.inner.config.hostname && node != "init-test.nanocl.io"
      })
    }) {
      return Err(IoError::invalid_input(
        "CargoUpdate".to_owned(),
        "Distributed Cargo rollout is not yet supported; desired state was not changed"
          .to_owned(),
      )
      .into());
    }
    let status = ObjPsStatusDb::read_by_pk(pk, &state.inner.pool).await?;
    let new_status = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Start.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(ObjPsStatusKind::Updating.to_string()),
      prev_actual: Some(status.actual),
      health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
    };
    CargoDb::update_from_spec(pk, &obj.spec, &obj.version, &state.inner.pool)
      .await
      .map_err(HttpError::from)?;
    ObjPsStatusDb::update_pk(pk, new_status, &state.inner.pool).await?;
    CargoDb::transform_read_by_pk(pk, &state.inner.pool)
      .await
      .map_err(HttpError::from)
  }
}

impl ObjPatchByPk for CargoDb {
  type ObjPatchIn = CargoObjPatchIn;
  type ObjPatchOut = Cargo;

  async fn fn_patch_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPatchOut> {
    let cargo = CargoDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let current = SpecDb::read_by_pk(&cargo.spec.key, &state.inner.pool)
      .await?
      .try_to_cargo_spec()?;
    let spec = apply_cargo_spec_patch(current, &obj.spec).map_err(|error| {
      IoError::invalid_input("CargoSpec".to_owned(), error.to_string())
    })?;
    let obj = &CargoObjPutIn {
      spec,
      version: obj.version.to_owned(),
    };
    CargoDb::fn_put_obj_by_pk(pk, obj, state).await
  }
}

impl ObjInspectByPk for CargoDb {
  type ObjInspectOut = CargoInspect;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &SystemState,
  ) -> HttpResult<Self::ObjInspectOut> {
    let cargo = CargoDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let processes =
      ProcessDb::read_by_kind_key(pk, None, &state.inner.pool).await?;
    let (_, _, _, running_instances) =
      utils::container::generic::count_status(&processes);
    let status = ObjPsStatusDb::read_by_pk(pk, &state.inner.pool).await?;
    Ok(CargoInspect {
      created_at: cargo.created_at,
      namespace_name: cargo.namespace_name,
      instance_total: processes.len(),
      instance_running: running_instances,
      spec: cargo.spec,
      instances: processes,
      status: status
        .try_into()
        .map_err(HttpError::internal_server_error)?,
    })
  }
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::{
    cargo_spec::{
      CargoNetworkMode, CargoPlacementSpec, CargoPlacementStrategy,
      CargoResourceRequirementSpec, Config, ContainerSpec,
    },
    generic::ImagePullPolicy,
  };

  use super::*;

  fn container(name: &str, image: &str) -> ContainerSpec {
    ContainerSpec {
      name: name.to_owned(),
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: ImagePullPolicy::IfNotPresent,
      container_config: Config {
        image: Some(image.to_owned()),
        ..Default::default()
      },
    }
  }

  fn current_spec() -> CargoSpec {
    CargoSpec {
      name: "patch-test".to_owned(),
      metadata: Some(serde_json::json!({"preserved": true})),
      replicas: 1,
      secrets: vec!["old-shared".to_owned()],
      init_containers: vec![container("old-init", "old-init:1")],
      containers: vec![
        container("old-api", "old-api:1"),
        container("old-worker", "old-worker:1"),
      ],
      ..Default::default()
    }
  }

  #[test]
  fn patch_replaces_every_present_field_as_a_whole() {
    let current = current_spec();
    let replacement_metadata = serde_json::json!({"replacement": true});
    let replacement_network = CargoNetworkMode::new("private").unwrap();
    let replacement_ports = std::collections::HashMap::from([(
      "8080/tcp".to_owned(),
      Some(vec![bollard_next::models::PortBinding {
        host_ip: Some("127.0.0.1".to_owned()),
        host_port: Some("18080".to_owned()),
      }]),
    )]);
    let replacement_init = vec![container("new-init", "new-init:2")];
    let replacement_containers = vec![container("new-api", "new-api:2")];
    let replacement_secrets =
      vec!["new-shared-a".to_owned(), "new-shared-b".to_owned()];
    let replacement_placement = CargoPlacementSpec {
      strategy: Some(CargoPlacementStrategy::Distinct),
      regions: Some(vec!["eu-west".to_owned()]),
      ..Default::default()
    };
    let replacement_resources = CargoResourceRequirementSpec {
      cpu_cores: Some(2),
      cpu_utilization_cap: Some(0.75),
      memory_bytes: Some(512 * 1024 * 1024),
      memory_utilization_cap: Some(0.8),
      cpu_weight: Some(0.6),
      memory_weight: Some(0.4),
      storage_bytes: Some(1024 * 1024 * 1024),
    };
    let patch = CargoSpecPatch {
      metadata: Some(replacement_metadata.clone()),
      replicas: Some(4),
      network_mode: Some(replacement_network.clone()),
      port_bindings: Some(replacement_ports.clone()),
      hostname: Some("replacement-api".to_owned()),
      dns: Some(vec!["10.42.0.1".to_owned()]),
      secrets: Some(replacement_secrets.clone()),
      placement: Some(replacement_placement.clone()),
      resource_requirement: Some(replacement_resources.clone()),
      init_containers: Some(replacement_init.clone()),
      containers: Some(replacement_containers.clone()),
      ..Default::default()
    };

    let patched = apply_cargo_spec_patch(current, &patch).unwrap();

    assert_eq!(patched.name, "patch-test");
    assert_eq!(patched.metadata, Some(replacement_metadata));
    assert_eq!(patched.replicas, 4);
    assert_eq!(patched.network_mode, Some(replacement_network));
    assert_eq!(patched.port_bindings, Some(replacement_ports));
    assert_eq!(patched.hostname.as_deref(), Some("replacement-api"));
    assert_eq!(patched.dns, Some(vec!["10.42.0.1".to_owned()]));
    assert_eq!(patched.secrets, replacement_secrets);
    assert_eq!(patched.placement, Some(replacement_placement));
    assert_eq!(patched.resource_requirement, Some(replacement_resources));
    assert_eq!(patched.init_containers, replacement_init);
    assert_eq!(patched.containers, replacement_containers);
  }

  #[test]
  fn patched_complete_declaration_is_revalidated() {
    let error = apply_cargo_spec_patch(
      current_spec(),
      &CargoSpecPatch {
        containers: Some(Vec::new()),
        ..Default::default()
      },
    )
    .unwrap_err();
    assert_eq!(error, CargoSpecValidationError::EmptyContainers);

    let error = apply_cargo_spec_patch(
      current_spec(),
      &CargoSpecPatch {
        replicas: Some(0),
        ..Default::default()
      },
    )
    .unwrap_err();
    assert_eq!(error, CargoSpecValidationError::InvalidReplicaCount);

    let error = apply_cargo_spec_patch(
      current_spec(),
      &CargoSpecPatch {
        containers: Some(vec![container("_sandbox", "nginx")]),
        ..Default::default()
      },
    )
    .unwrap_err();
    assert_eq!(
      error,
      CargoSpecValidationError::ReservedContainerName {
        path: "Containers[_sandbox]".to_owned(),
        name: "_sandbox".to_owned(),
      }
    );
  }

  #[test]
  fn unsupported_scale_and_placement_changes_are_rejected_before_persistence() {
    let current = current_spec();
    let mut next = current.clone();
    next.containers = vec![container("replacement", "replacement:2")];
    assert!(validate_supported_rollout_change(&current, &next).is_ok());

    next.replicas = 2;
    let error = validate_supported_rollout_change(&current, &next).unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(error.context(), Some("CargoUpdate"));

    next.replicas = current.replicas;
    next.placement = Some(Default::default());
    let error = validate_supported_rollout_change(&current, &next).unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(error.context(), Some("CargoUpdate"));
  }
}
