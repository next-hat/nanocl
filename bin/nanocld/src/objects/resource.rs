use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::resource::{ResourcePartial, Resource};

use crate::{
  utils,
  repositories::generic::*,
  models::{ResourceDb, SystemState, SpecDb},
};

use super::generic::*;

impl ObjCreate for ResourceDb {
  type ObjCreateIn = ResourcePartial;
  type ObjCreateOut = Resource;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    if ResourceDb::transform_read_by_pk(&obj.name, &state.pool)
      .await
      .is_ok()
    {
      return Err(HttpError::conflict(format!(
        "Resource {} already exists",
        &obj.name
      )));
    }
    let obj = utils::resource::hook_create(obj, &state.pool).await?;
    let resource = ResourceDb::create_from_spec(&obj, &state.pool).await?;
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
    let resource = ResourceDb::transform_read_by_pk(key, &state.pool).await?;
    if let Err(err) = utils::resource::hook_delete(&resource, &state.pool).await
    {
      log::warn!("{err}");
    }
    ResourceDb::del_by_pk(&resource.spec.resource_key, &state.pool).await?;
    SpecDb::del_by_kind_key(&resource.spec.resource_key, &state.pool).await?;
    Ok(resource)
  }
}
