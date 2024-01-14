use nanocl_error::http::HttpResult;

mod create;
mod delete;
mod patch;
mod put;
mod inspect;
mod process;

pub use create::*;
pub use delete::*;
pub use patch::*;
pub use put::*;
pub use inspect::*;
pub use process::*;

use crate::models::SystemState;

pub trait StateAction {
  type StateActionOut;

  async fn fn_action(
    &self,
    state: &SystemState,
  ) -> HttpResult<Self::StateActionOut>;
}
