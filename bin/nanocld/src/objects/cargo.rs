use futures_util::{stream::FuturesUnordered, StreamExt};
use bollard_next::container::RemoveContainerOptions;

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::{
  cargo::{Cargo, CargoDeleteQuery},
  cargo_spec::ReplicationMode,
};

use crate::{
  utils,
  repositories::generic::*,
  models::{CargoDb, SystemState, CargoObjCreateIn, ProcessDb, SpecDb},
};

use super::generic::*;

impl ObjCreate for CargoDb {
  type ObjCreateIn = CargoObjCreateIn;
  type ObjCreateOut = Cargo;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    let cargo = CargoDb::create_from_spec(
      &obj.namespace,
      &obj.spec,
      &obj.version,
      &state.pool,
    )
    .await?;
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
    if let Err(err) =
      utils::cargo::create_instances(&cargo, number, state).await
    {
      CargoDb::del_by_pk(&cargo.spec.cargo_key, &state.pool).await?;
      return Err(err);
    }
    Ok(cargo)
  }
}

impl ObjDelByPk for CargoDb {
  type ObjDelOut = Cargo;
  type ObjDelOpts = CargoDeleteQuery;

  async fn fn_del_obj_by_pk(
    key: &str,
    opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let cargo = CargoDb::transform_read_by_pk(key, &state.pool).await?;
    let processes =
      ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.pool).await?;
    processes
      .into_iter()
      .map(|process| async move {
        utils::process::remove(
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
      .collect::<Vec<Result<(), HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<_>, _>>()?;
    CargoDb::del_by_pk(key, &state.pool).await?;
    SpecDb::del_by_kind_key(key, &state.pool).await?;
    Ok(cargo)
  }
}
