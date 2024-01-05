use nanocl_error::http::HttpResult;
use nanocl_stubs::system::{EventActor, NativeEventAction};

use crate::utils;

use crate::models::SystemState;

pub trait ObjDelByPk {
  type ObjDelOut;
  type ObjDelOpts;

  async fn fn_del_obj_by_pk(
    key: &str,
    opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut>;

  async fn del_obj_by_pk(
    key: &str,
    opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut>
  where
    Self::ObjDelOut: Into<EventActor> + Clone,
  {
    let obj = Self::fn_del_obj_by_pk(key, opts, state).await?;
    utils::event_emitter::emit_normal_native_action(
      &obj,
      NativeEventAction::Delete,
      state,
    );
    Ok(obj)
  }
}
