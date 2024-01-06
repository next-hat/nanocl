use nanocl_error::http::HttpResult;
use nanocl_stubs::system::{EventActor, NativeEventAction};

use crate::models::SystemState;

/// A Create trait for all objects in Nanocl
/// It will automatically emit events
/// when an object is created,  etc.deleted, updated
/// You need to implement the `fn_create_obj` function
/// That will perform the create action and return the object
/// Then you can use the `create_obj` function
pub trait ObjCreate {
  type ObjCreateIn;
  type ObjCreateOut;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut>;

  async fn create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut>
  where
    Self::ObjCreateOut: Into<EventActor> + Clone,
  {
    let obj = Self::fn_create_obj(obj, state).await?;
    state.emit_normal_native_action(&obj, NativeEventAction::Create);
    Ok(obj)
  }
}
