use nanocl_error::io::IoResult;
use nanocl_stubs::cargo::{Cargo, CargoDeleteQuery};

use crate::{
  utils,
  objects::generic::*,
  models::{CargoDb, SystemState, CargoObjCreateIn},
};

impl ObjCreate for CargoDb {
  type ObjCreateIn = CargoObjCreateIn;
  type ObjCreateOut = Cargo;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> IoResult<Self::ObjCreateOut> {
    let cargo =
      utils::cargo::create(&obj.namespace, &obj.spec, &obj.version, state)
        .await?;
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
  ) -> IoResult<Self::ObjDelOut> {
    let cargo = utils::cargo::delete_by_key(key, opts.force, state).await?;
    Ok(cargo)
  }
}
