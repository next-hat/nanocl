use nanocl_error::io::IoResult;

use crate::models::{ObjTask, SystemState};

pub trait ObjTaskStart {
  async fn create_start_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask>;
}

pub trait ObjTaskDelete {
  async fn create_delete_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask>;
}

pub trait ObjTaskUpdate {
  async fn create_update_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask>;
}

pub trait ObjTaskStop {
  async fn create_stop_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask>;
}
