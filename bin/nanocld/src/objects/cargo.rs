/// Handle object creation, deletion, update, read and inspect
/// For a cargo object in the database.
/// An object will emit an event when it is created, updated or deleted.
///
use bollard_next::{container::Config, service::HostConfig};

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  cargo::{Cargo, CargoDeleteQuery, CargoInspect},
  cargo_spec::CargoSpecPartial,
  system::{NativeEventAction, ObjPsStatusKind, ObjPsStatusPartial},
};

use crate::{
  models::{
    CargoDb, CargoObjCreateIn, CargoObjPatchIn, CargoObjPutIn, ObjPsStatusDb,
    ObjPsStatusUpdate, ProcessDb, SpecDb, SystemState,
  },
  repositories::generic::*,
  utils,
};

use super::generic::*;

impl ObjCreate for CargoDb {
  type ObjCreateIn = CargoObjCreateIn;
  type ObjCreateOut = Cargo;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
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
    let key = utils::key::gen_key(&obj.namespace, &obj.spec.name);
    let new_spec =
      SpecDb::try_from_cargo_partial(&key, &obj.version, &obj.spec)?;
    let spec = SpecDb::create_from(new_spec, &state.inner.pool)
      .await?
      .try_to_cargo_spec()?;
    let status = ObjPsStatusPartial {
      key: key.clone(),
      wanted: ObjPsStatusKind::Create,
      prev_wanted: ObjPsStatusKind::Create,
      actual: ObjPsStatusKind::Create,
      prev_actual: ObjPsStatusKind::Create,
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
    let processes =
      ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.inner.pool)
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
    let status = ObjPsStatusDb::read_by_pk(pk, &state.inner.pool).await?;
    let new_status = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Start.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(ObjPsStatusKind::Updating.to_string()),
      prev_actual: Some(status.actual),
    };
    ObjPsStatusDb::update_pk(pk, new_status, &state.inner.pool).await?;
    CargoDb::update_from_spec(pk, &obj.spec, &obj.version, &state.inner.pool)
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
    let container = if let Some(container) = obj.spec.container.clone() {
      // merge env and ensure no duplicate key
      let new_env = container.env.unwrap_or_default();
      let mut env_vars: Vec<String> =
        cargo.spec.container.env.unwrap_or_default();
      // Merge environment variables from new_env into the merged array
      for env_var in new_env {
        let parts: Vec<&str> = env_var.split('=').collect();
        if parts.len() < 2 {
          continue;
        }
        let name = parts[0].to_owned();
        let value = parts[1..].join("=");
        if let Some(pos) = env_vars
          .iter()
          .position(|x| x.starts_with(&format!("{name}=")))
        {
          let old_value = env_vars[pos].to_owned();
          log::trace!(
            "env var: {name} old_value: {old_value} new_value: {value}"
          );
          if old_value != value && !value.is_empty() {
            // Update the value if it has changed
            env_vars[pos] = format!("{}={}", name, value);
          } else if value.is_empty() {
            // Remove the variable if the value is empty
            env_vars.remove(pos);
          }
        } else {
          // Add new environment variables
          env_vars.push(env_var);
        }
      }
      // merge volumes and ensure no duplication
      let new_volumes = container
        .host_config
        .clone()
        .unwrap_or_default()
        .binds
        .unwrap_or_default();
      let mut volumes: Vec<String> = cargo
        .spec
        .container
        .host_config
        .clone()
        .unwrap_or_default()
        .binds
        .unwrap_or_default();
      for volume in new_volumes {
        if !volumes.contains(&volume) {
          volumes.push(volume);
        }
      }
      let image = if let Some(image) = container.image.clone() {
        Some(image)
      } else {
        cargo.spec.container.image
      };
      let cmd = if let Some(cmd) = container.cmd {
        Some(cmd)
      } else {
        cargo.spec.container.cmd
      };
      Config {
        cmd,
        image,
        env: Some(env_vars),
        host_config: if !volumes.is_empty()
          || cargo.spec.container.host_config.is_some()
        {
          Some(HostConfig {
            binds: Some(volumes),
            ..cargo.spec.container.host_config.unwrap_or_default()
          })
        } else {
          None
        },
        ..cargo.spec.container
      }
    } else {
      cargo.spec.container
    };
    let spec = CargoSpecPartial {
      name: cargo.spec.name.clone(),
      container,
      init_container: if obj.spec.init_container.is_some() {
        obj.spec.init_container.clone()
      } else {
        cargo.spec.init_container
      },
      replication: obj.spec.replication.clone(),
      secrets: if obj.spec.secrets.is_some() {
        obj.spec.secrets.clone()
      } else {
        cargo.spec.secrets
      },
      metadata: if obj.spec.metadata.is_some() {
        obj.spec.metadata.clone()
      } else {
        cargo.spec.metadata
      },
      image_pull_secret: if obj.spec.image_pull_secret.is_some() {
        obj.spec.image_pull_secret.clone()
      } else {
        cargo.spec.image_pull_secret
      },
      image_pull_policy: if obj.spec.image_pull_policy.is_some() {
        obj.spec.image_pull_policy.clone()
      } else {
        cargo.spec.image_pull_policy
      },
    };
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
    let processes = ProcessDb::read_by_kind_key(pk, &state.inner.pool).await?;
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
