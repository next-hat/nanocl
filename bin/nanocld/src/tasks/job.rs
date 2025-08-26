use nanocl_error::io::IoError;
use nanocl_stubs::process::ProcessKind;

use crate::{
  models::{JobDb, SystemState},
  utils,
};

use super::generic::*;

impl ObjTask for JobDb {
  fn get_kind() -> String {
    "job".to_owned()
  }
}

impl ObjTaskStart for JobDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = JobDb::create_mutex(&key, "start", &state).await?;
      utils::container::job::start(&key, &state).await?;
      JobDb::delete_mutex(&mutex.key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskDelete for JobDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = JobDb::create_mutex(&key, "delete", &state).await?;
      utils::container::job::delete(&key, &state).await?;
      JobDb::delete_mutex(&mutex.key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskStop for JobDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = JobDb::create_mutex(&key, "stop", &state).await?;
      utils::container::process::stop_instances(
        &key,
        &ProcessKind::Job,
        &state,
      )
      .await?;
      JobDb::delete_mutex(&mutex.key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}
