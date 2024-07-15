use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  resource::{Resource, ResourcePartial},
  system::NativeEventAction,
};

use crate::{
  models::{ResourceDb, SpecDb, SystemState},
  repositories::generic::*,
};

use super::generic::*;

impl ObjCreate for ResourceDb {
  type ObjCreateIn = ResourcePartial;
  type ObjCreateOut = Resource;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    if ResourceDb::transform_read_by_pk(&obj.name, &state.inner.pool)
      .await
      .is_ok()
    {
      return Err(HttpError::conflict(format!(
        "Resource {} already exists",
        &obj.name
      )));
    }
    let obj = ResourceDb::hook_create(obj, &state.inner.pool).await?;
    let resource =
      ResourceDb::create_from_spec(&obj, &state.inner.pool).await?;
    Ok(resource)
  }
}

impl ObjDelByPk for ResourceDb {
  type ObjDelOut = Resource;
  type ObjDelOpts = ();

  async fn fn_del_obj_by_pk(
    key: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let resource =
      ResourceDb::transform_read_by_pk(key, &state.inner.pool).await?;
    if let Err(err) =
      ResourceDb::hook_delete(&resource, &state.inner.pool).await
    {
      log::warn!("{err}");
    }
    ResourceDb::del_by_pk(&resource.spec.resource_key, &state.inner.pool)
      .await?;
    SpecDb::del_by_kind_key(&resource.spec.resource_key, &state.inner.pool)
      .await?;
    Ok(resource)
  }
}

impl ObjPutByPk for ResourceDb {
  type ObjPutIn = ResourcePartial;
  type ObjPutOut = Resource;

  fn get_put_event() -> NativeEventAction {
    NativeEventAction::Update
  }

  async fn fn_put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut> {
    ResourceDb::read_by_pk(pk, &state.inner.pool).await?;
    let resource = ResourceDb::hook_create(obj, &state.inner.pool).await?;
    let resource =
      ResourceDb::update_from_spec(&resource, &state.inner.pool).await?;
    Ok(resource)
  }
}
