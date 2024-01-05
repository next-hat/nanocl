use nanocl_error::io::IoResult;
use nanocl_stubs::secret::{Secret, SecretPartial, SecretUpdate};

use crate::{
  objects::generic::*,
  repositories::generic::*,
  models::{SecretDb, SystemState},
};

impl ObjCreate for SecretDb {
  type ObjCreateIn = SecretPartial;
  type ObjCreateOut = Secret;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> IoResult<Self::ObjCreateOut> {
    let secret = SecretDb::create_from(obj, &state.pool).await?;
    let secret: Secret = secret.try_into()?;
    Ok(secret)
  }
}

impl ObjDelByPk for SecretDb {
  type ObjDelOut = Secret;
  type ObjDelOpts = ();

  async fn fn_del_obj_by_pk(
    key: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> IoResult<Self::ObjDelOut> {
    let secret = SecretDb::transform_read_by_pk(key, &state.pool).await?;
    SecretDb::del_by_pk(key, &state.pool).await?;
    Ok(secret)
  }
}

impl ObjPatchByPk for SecretDb {
  type ObjPatchIn = SecretUpdate;
  type ObjPatchOut = Secret;

  async fn fn_patch_obj_by_pk(
    key: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> IoResult<Self::ObjPatchOut> {
    SecretDb::update_pk(key, obj, &state.pool).await?.try_into()
  }
}
