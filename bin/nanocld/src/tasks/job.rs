use nanocl_error::io::IoError;
use nanocl_stubs::process::ProcessKind;

use crate::{
  models::{JobDb, SystemState},
  utils,
};

use super::generic::*;

impl ObjTaskStart for JobDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::job::start(&key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskDelete for JobDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::job::delete(&key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskStop for JobDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::process::stop_instances(
        &key,
        &ProcessKind::Job,
        &state,
      )
      .await?;
      Ok::<_, IoError>(())
    })
  }
}
