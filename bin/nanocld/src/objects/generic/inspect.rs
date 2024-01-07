use nanocl_error::http::HttpResult;

use crate::models::SystemState;

pub trait ObjInspectByPk {
  type ObjInspectOut;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &SystemState,
  ) -> HttpResult<Self::ObjInspectOut>;
}
