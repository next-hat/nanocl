use nanocl_error::io::IoError;
use nanocl_stubs::process::ProcessKind;

use crate::{
  models::{CargoDb, SystemState},
  utils,
};

use super::generic::*;

impl ObjTask for CargoDb {
  fn get_kind() -> String {
    "cargo".to_owned()
  }
}

impl ObjTaskStart for CargoDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = CargoDb::create_mutex(&key, "start", &state).await?;
      utils::container::cargo::start(&key, &state).await?;
      CargoDb::delete_mutex(&mutex.key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskDelete for CargoDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = CargoDb::create_mutex(&key, "delete", &state).await?;
      utils::container::cargo::delete(&key, &state).await?;
      CargoDb::delete_mutex(&mutex.key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskUpdate for CargoDb {
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = CargoDb::create_mutex(&key, "update", &state).await?;
      utils::container::cargo::update(&key, &state).await?;
      CargoDb::delete_mutex(&mutex.key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskStop for CargoDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = CargoDb::create_mutex(&key, "stop", &state).await?;
      utils::container::process::stop_instances(
        &key,
        &ProcessKind::Cargo,
        &state,
      )
      .await?;
      CargoDb::delete_mutex(&mutex.key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}
