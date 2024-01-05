use nanocl_error::http::HttpResult;
use nanocl_stubs::system::{EventActor, NativeEventAction};

use crate::models::SystemState;

pub trait ObjPatchByPk {
  type ObjPatchIn;
  type ObjPatchOut;

  async fn fn_patch_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPatchOut>;

  async fn patch_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPatchOut>
  where
    Self::ObjPatchOut: Into<EventActor> + Clone,
  {
    let obj = Self::fn_patch_obj_by_pk(pk, obj, state).await?;
    state.emit_normal_native_action(&obj, NativeEventAction::Update);
    Ok(obj)
  }
}
