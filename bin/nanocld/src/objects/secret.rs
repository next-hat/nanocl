use nanocl_error::http::HttpResult;
use nanocl_stubs::secret::{Secret, SecretPartial, SecretUpdate};

use crate::{
  repositories::generic::*,
  models::{SecretDb, SystemState},
};

use super::generic::*;

impl ObjCreate for SecretDb {
  type ObjCreateIn = SecretPartial;
  type ObjCreateOut = Secret;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    let secret = SecretDb::create_from(obj, &state.inner.pool).await?;
    let secret: Secret = secret.try_into()?;
    Ok(secret)
  }
}

impl ObjDelByPk for SecretDb {
  type ObjDelOut = Secret;
  type ObjDelOpts = ();

  async fn fn_del_obj_by_pk(
    pk: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let secret = SecretDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    SecretDb::del_by_pk(pk, &state.inner.pool).await?;
    Ok(secret)
  }
}

impl ObjPatchByPk for SecretDb {
  type ObjPatchIn = SecretUpdate;
  type ObjPatchOut = Secret;

  async fn fn_patch_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPatchOut> {
    let secret = SecretDb::update_pk(pk, obj, &state.inner.pool)
      .await?
      .try_into()?;
    Ok(secret)
  }
}
