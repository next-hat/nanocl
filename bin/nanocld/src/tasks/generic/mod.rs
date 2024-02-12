use nanocl_error::io::IoResult;

use crate::models::{ObjTask, SystemState};

pub trait ObjTaskStart {
  async fn start(key: &str, state: &SystemState) -> IoResult<ObjTask>;
}

pub trait ObjTaskDelete {
  async fn delete(key: &str, state: &SystemState) -> IoResult<ObjTask>;
}

pub trait ObjTaskUpdate {
  async fn update(key: &str, state: &SystemState) -> IoResult<ObjTask>;
}

pub trait ObjTaskStop {
  async fn stop(key: &str, state: &SystemState) -> IoResult<ObjTask>;
}
