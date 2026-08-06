use std::{collections::BTreeMap, future::Future};

use nanocl_error::io::IoError;
use nanocl_stubs::{process::ProcessKind, system::EventActorKind};

use crate::{
  models::{CargoDb, CargoReplicaAssignment, CargoReplicaDb, SystemState},
  repositories::generic::*,
  utils::{
    self,
    container::generic::{SelectionAssignment, SelectionPlan},
  },
};

use super::generic::*;

impl ObjTask for CargoDb {
  fn get_kind() -> String {
    EventActorKind::Cargo.to_string()
  }
}

fn group_assigned_replicas_by_node(
  cargo_key: &str,
  assignments: &[CargoReplicaAssignment],
  selection: &SelectionPlan,
) -> Result<Vec<SelectionAssignment>, IoError> {
  let mut counts = BTreeMap::<String, usize>::new();
  for assignment in assignments {
    if let Some(node_name) = &assignment.node_name {
      *counts.entry(node_name.clone()).or_default() += 1;
    }
  }
  let dispatches = selection
    .assignments
    .iter()
    .filter_map(|assignment| {
      let replicas = counts.remove(&assignment.node.name)?;
      Some(SelectionAssignment {
        node: assignment.node.clone(),
        replicas,
      })
    })
    .collect::<Vec<_>>();
  if let Some((node_name, _)) = counts.into_iter().next() {
    return Err(IoError::other(
      "CargoScheduling",
      &format!(
        "Cargo {cargo_key:?} persisted an assignment for node {node_name:?} outside its placement result"
      ),
    ));
  }
  Ok(dispatches)
}

async fn reconcile_and_dispatch<R, RFut, D, DFut>(
  cargo_key: &str,
  selection: SelectionPlan,
  reconcile: R,
  mut dispatch: D,
) -> Result<Vec<CargoReplicaAssignment>, IoError>
where
  R: FnOnce(String, SelectionPlan) -> RFut,
  RFut: Future<Output = Result<Vec<CargoReplicaAssignment>, IoError>>,
  D: FnMut(SelectionAssignment) -> DFut,
  DFut: Future<Output = Result<(), IoError>>,
{
  // The repository transaction commits before this future returns. Only the
  // returned persisted assignments are used to derive dispatch counts.
  let assignments = reconcile(cargo_key.to_owned(), selection.clone()).await?;
  let dispatches =
    group_assigned_replicas_by_node(cargo_key, &assignments, &selection)?;
  for assignment in dispatches {
    dispatch(assignment).await?;
  }
  Ok(assignments)
}

async fn dispatch_start(
  cargo: nanocl_stubs::cargo::Cargo,
  cargo_key: String,
  assignment: SelectionAssignment,
  state: SystemState,
) -> Result<(), IoError> {
  if assignment.node.name == state.inner.config.hostname
    || assignment.node.name == "init-test.nanocl.io"
  {
    utils::container::cargo::start(&cargo, assignment.replicas, &state).await?;
  } else {
    let client_opts = nanocld_client::ConnectOpts {
      url: assignment.node.endpoint,
      ssl: None,
      version: Some(assignment.node.version),
    };
    let client = nanocld_client::NanocldClient::connect_to(&client_opts)?;
    let params = nanocl_stubs::node::StartNodeCargoParams {
      cargo_key,
      replicas: assignment.replicas,
    };
    client.start_node_cargo(&params).await?;
  }
  Ok(())
}

impl ObjTaskStart for CargoDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = CargoDb::create_mutex(&key, "start", &state).await?;
      let cargo =
        CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let selection =
        utils::container::generic::plan_selection(&cargo, &state).await?;
      let cargo_key = key.clone();
      let dispatch_cargo = cargo.clone();
      let dispatch_cargo_key = key.clone();
      let dispatch_state = state.clone();
      let pool = state.inner.pool.clone();
      reconcile_and_dispatch(
        &cargo_key,
        selection,
        move |cargo_key, selection| async move {
          CargoReplicaDb::reconcile_assignments(&cargo_key, &selection, &pool)
            .await
        },
        move |assignment| {
          dispatch_start(
            dispatch_cargo.clone(),
            dispatch_cargo_key.clone(),
            assignment,
            dispatch_state.clone(),
          )
        },
      )
      .await?;
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

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use crate::{models::NodeDb, vars};

  use super::*;

  fn node(name: &str) -> NodeDb {
    NodeDb {
      name: name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      endpoint: format!("tcp://{name}:8585"),
      version: vars::VERSION.to_owned(),
      metadata: None,
    }
  }

  fn selection(
    total_replicas: usize,
    assignments: &[(&str, usize)],
  ) -> SelectionPlan {
    SelectionPlan {
      total_replicas,
      assignments: assignments
        .iter()
        .map(|(node_name, replicas)| SelectionAssignment {
          node: node(node_name),
          replicas: *replicas,
        })
        .collect(),
    }
  }

  fn assignment(
    ordinal: i32,
    node_name: Option<&str>,
  ) -> CargoReplicaAssignment {
    CargoReplicaAssignment {
      replica_key: uuid::Uuid::new_v4(),
      ordinal,
      node_name: node_name.map(str::to_owned),
    }
  }

  #[test]
  fn assigned_replica_grouping_excludes_nulls_and_uses_persisted_counts() {
    let selection = selection(4, &[("node-b", 1), ("node-a", 2)]);
    // Deliberately differ from the planner quotas to prove this adapter uses
    // only the committed durable assignments as its count source.
    let assignments = vec![
      assignment(0, Some("node-a")),
      assignment(1, Some("node-b")),
      assignment(2, Some("node-b")),
      assignment(3, None),
    ];

    let grouped = group_assigned_replicas_by_node(
      "global.scheduler",
      &assignments,
      &selection,
    )
    .unwrap();
    assert_eq!(
      grouped
        .iter()
        .map(|assignment| (assignment.node.name.as_str(), assignment.replicas))
        .collect::<Vec<_>>(),
      [("node-b", 2), ("node-a", 1)],
      "dispatch counts must come from persisted assigned replicas"
    );
    assert_eq!(
      grouped.iter().map(|item| item.replicas).sum::<usize>(),
      3,
      "the unassigned desired replica must not be dispatched"
    );
  }

  #[ntex::test]
  async fn scheduling_reconciles_before_dispatching_persisted_counts() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured = Arc::new(Mutex::new(Vec::<(String, usize)>::new()));
    let reconcile_events = events.clone();
    let dispatch_events = events.clone();
    let dispatch_captured = captured.clone();
    let persisted = vec![
      assignment(0, Some("node-a")),
      assignment(1, Some("node-b")),
      assignment(2, None),
    ];
    let returned = persisted.clone();

    let result = reconcile_and_dispatch(
      "global.scheduler",
      selection(3, &[("node-b", 1), ("node-a", 1)]),
      move |cargo_key, selection| {
        let events = reconcile_events.clone();
        let assignments = returned.clone();
        async move {
          assert_eq!(cargo_key, "global.scheduler");
          assert_eq!(selection.total_replicas, 3);
          assert_eq!(
            selection
              .assignments
              .iter()
              .map(|assignment| {
                (assignment.node.name.as_str(), assignment.replicas)
              })
              .collect::<Vec<_>>(),
            [("node-b", 1), ("node-a", 1)]
          );
          events.lock().unwrap().push("reconcile".to_owned());
          Ok(assignments)
        }
      },
      move |assignment| {
        let events = dispatch_events.clone();
        let captured = dispatch_captured.clone();
        async move {
          events
            .lock()
            .unwrap()
            .push(format!("dispatch:{}", assignment.node.name));
          captured
            .lock()
            .unwrap()
            .push((assignment.node.name, assignment.replicas));
          Ok(())
        }
      },
    )
    .await
    .unwrap();

    assert_eq!(result, persisted);
    assert_eq!(
      *events.lock().unwrap(),
      ["reconcile", "dispatch:node-b", "dispatch:node-a"]
    );
    assert_eq!(
      *captured.lock().unwrap(),
      [("node-b".to_owned(), 1), ("node-a".to_owned(), 1)]
    );
  }

  #[ntex::test]
  async fn scheduling_repository_failure_prevents_dispatch() {
    let dispatches = Arc::new(Mutex::new(0usize));
    let observed_dispatches = dispatches.clone();
    let error = reconcile_and_dispatch(
      "global.scheduler",
      selection(1, &[("node-a", 1)]),
      |_, _| async {
        Err(IoError::other(
          "CargoReplicaReconcile",
          "injected repository failure",
        ))
      },
      move |_| {
        let dispatches = observed_dispatches.clone();
        async move {
          *dispatches.lock().unwrap() += 1;
          Ok(())
        }
      },
    )
    .await
    .unwrap_err();

    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert_eq!(*dispatches.lock().unwrap(), 0);
  }

  #[test]
  fn assigned_replica_grouping_rejects_unknown_persisted_nodes() {
    let error = group_assigned_replicas_by_node(
      "global.scheduler",
      &[assignment(0, Some("node-b"))],
      &selection(1, &[("node-a", 1)]),
    )
    .unwrap_err();

    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert_eq!(error.context(), Some("CargoScheduling"));
    assert!(error.to_string().contains("node-b"));
  }
}
