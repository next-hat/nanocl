use nanocl_error::http::HttpResult;

use crate::models::SystemState;

pub trait StateAction {
  type StateActionOut;

  fn fn_action(
    &self,
    state: &SystemState,
  ) -> impl std::future::Future<Output = HttpResult<Self::StateActionOut>> + Send;
}
