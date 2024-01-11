/// Handle object creation, deletion, update, read and inspect
/// For a cargo object in the database.
/// An object will emit an event when it is created, updated or deleted.
///
use futures_util::{StreamExt, stream::FuturesUnordered};
use bollard_next::{
  service::HostConfig,
  container::{Config, RemoveContainerOptions},
};

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::{
  process::ProcessKind,
  cargo::{Cargo, CargoDeleteQuery, CargoInspect},
  cargo_spec::{ReplicationMode, CargoSpecPartial},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{
    CargoDb, SystemState, CargoObjCreateIn, ProcessDb, SpecDb, CargoObjPutIn,
    CargoObjPatchIn, ObjPsStatusPartial, ObjPsStatusKind, ObjPsStatusDb,
  },
};

use super::generic::*;

impl ObjProcess for CargoDb {
  fn get_process_kind() -> ProcessKind {
    ProcessKind::Cargo
  }
}

impl ObjCreate for CargoDb {
  type ObjCreateIn = CargoObjCreateIn;
  type ObjCreateOut = Cargo;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    // test if the name of the cargo include a . in the name and throw error if true
    if obj.spec.name.contains('.') {
      return Err(HttpError::bad_request("Cargo name cannot contain '.'"));
    }
    let key = utils::key::gen_key(&obj.namespace, &obj.spec.name);
    let new_spec =
      SpecDb::try_from_cargo_partial(&key, &obj.version, &obj.spec)?;
    let spec = SpecDb::create_from(new_spec, &state.pool)
      .await?
      .try_to_cargo_spec()?;
    let status = ObjPsStatusPartial {
      key: key.clone(),
      wanted: ObjPsStatusKind::Created,
      prev_wanted: ObjPsStatusKind::Created,
      actual: ObjPsStatusKind::Created,
      prev_actual: ObjPsStatusKind::Created,
    };
    ObjPsStatusDb::create_from(status, &state.pool).await?;
    let new_item = CargoDb {
      key: key.clone(),
      name: obj.spec.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: obj.namespace.clone(),
      status_key: key,
      spec_key: spec.key,
    };
    let cargo = CargoDb::create_from(new_item, &state.pool)
      .await?
      .with_spec(&spec);
    Ok(cargo)
  }
}

impl ObjDelByPk for CargoDb {
  type ObjDelOut = Cargo;
  type ObjDelOpts = CargoDeleteQuery;

  async fn fn_del_obj_by_pk(
    pk: &str,
    opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let cargo = CargoDb::transform_read_by_pk(pk, &state.pool).await?;
    let processes =
      ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.pool).await?;
    processes
      .into_iter()
      .map(|process| async move {
        CargoDb::del_process_by_pk(
          &process.key,
          Some(RemoveContainerOptions {
            force: opts.force.unwrap_or(false),
            ..Default::default()
          }),
          state,
        )
        .await
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<HttpResult<()>>>()
      .await
      .into_iter()
      .collect::<HttpResult<Vec<_>>>()?;
    CargoDb::clear_by_pk(pk, &state.pool).await?;
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
    let cargo =
      CargoDb::update_from_spec(pk, &obj.spec, &obj.version, &state.pool)
        .await?;
    // Get the number of instance to create
    let number = if let Some(mode) = &cargo.spec.replication {
      match mode {
        ReplicationMode::Static(replication_static) => {
          replication_static.number
        }
        ReplicationMode::Auto => 1,
        ReplicationMode::Unique => 1,
        ReplicationMode::UniqueByNode => 1,
        _ => 1,
      }
    } else {
      1
    };
    let processes = ProcessDb::read_by_kind_key(pk, &state.pool).await?;
    // Create instance with the new spec
    let mut error = None;
    let new_instances =
      match utils::cargo::create_instances(&cargo, number, state).await {
        Err(err) => {
          error = Some(err);
          Vec::default()
        }
        Ok(instances) => instances,
      };
    // start created containers
    match CargoDb::start_process_by_kind_key(pk, state).await {
      Err(err) => {
        log::error!(
          "Unable to start cargo instance {} : {err}",
          cargo.spec.cargo_key
        );
        utils::cargo::delete_instances(
          &new_instances
            .iter()
            .map(|i| i.key.clone())
            .collect::<Vec<_>>(),
          state,
        )
        .await?;
      }
      Ok(_) => {
        // Delete old containers
        utils::cargo::delete_instances(
          &processes.iter().map(|c| c.key.clone()).collect::<Vec<_>>(),
          state,
        )
        .await?;
      }
    }
    match error {
      Some(err) => Err(err),
      None => Ok(cargo),
    }
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
    let cargo = CargoDb::transform_read_by_pk(pk, &state.pool).await?;
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
        host_config: Some(HostConfig {
          binds: Some(volumes),
          ..cargo.spec.container.host_config.unwrap_or_default()
        }),
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
    let cargo = CargoDb::transform_read_by_pk(pk, &state.pool).await?;
    let processes = ProcessDb::read_by_kind_key(pk, &state.pool).await?;
    let (_, _, _, running_instances) = utils::process::count_status(&processes);
    Ok(CargoInspect {
      created_at: cargo.created_at,
      namespace_name: cargo.namespace_name,
      instance_total: processes.len(),
      instance_running: running_instances,
      spec: cargo.spec,
      instances: processes,
    })
  }
}
