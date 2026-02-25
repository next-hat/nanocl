use nanocl_error::io::IoError;
use nanocl_stubs::{process::ProcessKind, system::EventActorKind};

use crate::{
  models::{CargoDb, SystemState},
  repositories::generic::*,
  utils,
};

use super::generic::*;

impl ObjTask for CargoDb {
  fn get_kind() -> String {
    EventActorKind::Cargo.to_string()
  }
}

impl ObjTaskStart for CargoDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = CargoDb::create_mutex(&key, "start", &state).await?;
      let cargo =
        CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let plan =
        utils::container::generic::plan_selection(&cargo, &state).await?;
      for assignment in plan.assignments {
        let node_selected = assignment.node;
        let replicas = assignment.replicas;
        // If the node selected is the current node we start the cargo with internal utils
        // There is a edge case in the test environment where the hostname is always "init-test.nanocl.io" so we need to check for that as well
        if node_selected.name == state.inner.config.hostname
          || node_selected.name == "init-test.nanocl.io"
        {
          utils::container::cargo::start(&cargo, replicas, &state).await?;
        } else {
          let client_opts = nanocld_client::ConnectOpts {
            url: node_selected.endpoint.clone(),
            ssl: None,
            version: Some(node_selected.version.clone()),
          };
          let client = nanocld_client::NanocldClient::connect_to(&client_opts)?;
          let params = nanocl_stubs::node::StartNodeCargoParams {
            cargo_key: key.clone(),
            replicas,
          };
          client.start_node_cargo(&params).await?;
        }
      }
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
