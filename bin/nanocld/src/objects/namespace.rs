use bollard_next::network::{CreateNetworkOptions, InspectNetworkOptions};

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::namespace::{Namespace, NamespaceInspect, NamespacePartial};

use crate::{
  models::{CargoDb, NamespaceDb, SystemState},
  repositories::generic::*,
};

use super::generic::*;

impl ObjCreate for NamespaceDb {
  type ObjCreateIn = NamespacePartial;
  type ObjCreateOut = Namespace;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    if NamespaceDb::read_by_pk(&obj.name, &state.inner.pool)
      .await
      .is_ok()
    {
      return Err(HttpError::conflict(format!(
        "Namespace {}: already exist",
        &obj.name
      )));
    }
    if state
      .inner
      .docker_api
      .inspect_network(&obj.name, None::<InspectNetworkOptions<String>>)
      .await
      .is_ok()
    {
      let item = NamespaceDb::create_from(obj, &state.inner.pool).await?;
      return Ok(item.into());
    }
    let config = CreateNetworkOptions {
      name: obj.name.to_owned(),
      driver: String::from("bridge"),
      ..Default::default()
    };
    state.inner.docker_api.create_network(config).await?;
    let item = NamespaceDb::create_from(obj, &state.inner.pool)
      .await?
      .into();
    Ok(item)
  }
}

impl ObjInspectByPk for NamespaceDb {
  type ObjInspectOut = NamespaceInspect;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &SystemState,
  ) -> HttpResult<Self::ObjInspectOut> {
    let namespace = NamespaceDb::read_by_pk(pk, &state.inner.pool).await?;
    let models =
      CargoDb::read_by_namespace(&namespace.name, &state.inner.pool).await?;
    // TODO: Refactor this it doesn't scale at all
    let mut cargoes = Vec::new();
    for cargo in models {
      let cargo =
        CargoDb::inspect_obj_by_pk(&cargo.spec.cargo_key, state).await?;
      cargoes.push(cargo);
    }
    Ok(NamespaceInspect {
      name: namespace.name,
      cargoes,
    })
  }
}

impl ObjDelByPk for NamespaceDb {
  type ObjDelOpts = ();
  type ObjDelOut = Namespace;

  async fn fn_del_obj_by_pk(
    pk: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let item = NamespaceDb::read_by_pk(pk, &state.inner.pool).await?;
    CargoDb::delete_by_namespace(pk, state).await?;
    NamespaceDb::del_by_pk(pk, &state.inner.pool).await?;
    if let Err(err) = state.inner.docker_api.remove_network(pk).await {
      log::error!("Unable to remove network {} got error: {}", pk, err);
    }
    Ok(item.into())
  }
}
