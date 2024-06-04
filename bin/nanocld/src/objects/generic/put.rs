use nanocl_error::http::HttpResult;
use nanocl_stubs::system::{EventActor, NativeEventAction};

use crate::models::SystemState;

pub trait ObjPutByPk {
  type ObjPutIn;
  type ObjPutOut;

  fn get_put_event() -> NativeEventAction {
    NativeEventAction::Updating
  }

  async fn fn_put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut>;

  async fn put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut>
  where
    Self::ObjPutOut: Into<EventActor> + Clone,
  {
    let obj = Self::fn_put_obj_by_pk(pk, obj, state).await?;
    state
      .emit_normal_native_action_sync(&obj, Self::get_put_event())
      .await;
    Ok(obj)
  }
}
