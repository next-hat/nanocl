use nanocl_error::http::HttpResult;
use nanocl_stubs::system::{EventActor, NativeEventAction};

use crate::models::SystemState;

pub trait ObjDelByPk {
  type ObjDelOut;
  type ObjDelOpts;

  fn get_del_event() -> NativeEventAction {
    NativeEventAction::Destroy
  }

  async fn fn_del_obj_by_pk(
    pk: &str,
    opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut>;

  async fn del_obj_by_pk(
    pk: &str,
    opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut>
  where
    Self::ObjDelOut: Into<EventActor> + Clone,
  {
    let obj = Self::fn_del_obj_by_pk(pk, opts, state).await?;
    state.emit_normal_native_action(&obj, Self::get_del_event());
    Ok(obj)
  }
}
