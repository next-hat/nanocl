use bollard_next::network::{InspectNetworkOptions, CreateNetworkOptions};

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::namespace::{NamespacePartial, Namespace, NamespaceInspect};

use crate::{
  utils,
  repositories::generic::*,
  models::{CargoDb, NamespaceDb, SystemState},
};

use super::generic::*;

impl ObjCreate for NamespaceDb {
  type ObjCreateIn = NamespacePartial;
  type ObjCreateOut = Namespace;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    if NamespaceDb::read_by_pk(&obj.name, &state.pool)
      .await
      .is_ok()
    {
      return Err(HttpError::conflict(format!(
        "Namespace {}: already exist",
        &obj.name
      )));
    }
    let network_key = utils::key::gen_key(&state.config.hostname, &obj.name);
    if state
      .docker_api
      .inspect_network(&network_key, None::<InspectNetworkOptions<String>>)
      .await
      .is_ok()
    {
      let item = NamespaceDb::create_from(obj, &state.pool).await?;
      return Ok(item.into());
    }
    let config = CreateNetworkOptions {
      name: network_key,
      driver: String::from("bridge"),
      ..Default::default()
    };
    state.docker_api.create_network(config).await?;
    let item = NamespaceDb::create_from(obj, &state.pool).await?.into();
    Ok(item)
  }
}

impl ObjInspectByPk for NamespaceDb {
  type ObjInspectOut = NamespaceInspect;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &SystemState,
  ) -> HttpResult<Self::ObjInspectOut> {
    let namespace = NamespaceDb::read_by_pk(pk, &state.pool).await?;
    let models =
      CargoDb::read_by_namespace(&namespace.name, &state.pool).await?;
    let mut cargoes = Vec::new();
    for cargo in models {
      let cargo =
        CargoDb::inspect_obj_by_pk(&cargo.spec.cargo_key, state).await?;
      cargoes.push(cargo);
    }
    let network_key =
      utils::key::gen_key(&namespace.name, &state.config.hostname);
    let network = state
      .docker_api
      .inspect_network(&network_key, None::<InspectNetworkOptions<String>>)
      .await?;
    Ok(NamespaceInspect {
      name: namespace.name,
      cargoes,
      network,
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
    let item = NamespaceDb::read_by_pk(pk, &state.pool).await?;
    CargoDb::delete_by_namespace(pk, state).await?;
    NamespaceDb::del_by_pk(pk, &state.pool).await?;
    if let Err(err) = state.docker_api.remove_network(pk).await {
      log::error!("Unable to remove network {} got error: {}", pk, err);
    }
    Ok(item.into())
  }
}
