use std::{collections::BTreeMap, future::Future};

use nanocl_error::io::IoError;
use nanocl_stubs::{
  node::CargoReplicaTask,
  process::ProcessKind,
  system::{
    EventActorKind, NativeEventAction, ObjPsHealthStatusKind, ObjPsStatusKind,
  },
};

use crate::{
  models::{
    CargoDb, CargoReplicaAssignment, CargoReplicaDb, NodeDb, ObjPsStatusDb,
    ProcessDb, SystemState,
  },
  repositories::generic::*,
  utils::{self, container::generic::SelectionPlan},
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
) -> Result<Vec<(NodeDb, Vec<CargoReplicaTask>)>, IoError> {
  let mut replicas_by_node = BTreeMap::<String, Vec<CargoReplicaTask>>::new();
  for assignment in assignments {
    if let Some(node_name) = &assignment.node_name {
      replicas_by_node.entry(node_name.clone()).or_default().push(
        CargoReplicaTask {
          key: assignment.replica_key,
          ordinal: assignment.ordinal,
        },
      );
    }
  }
  for replicas in replicas_by_node.values_mut() {
    replicas.sort_by_key(|replica| replica.ordinal);
  }
  let dispatches = selection
    .assignments
    .iter()
    .filter_map(|assignment| {
      let replicas = replicas_by_node.remove(&assignment.node.name)?;
      Some((assignment.node.clone(), replicas))
    })
    .collect::<Vec<_>>();
  if let Some((node_name, _)) = replicas_by_node.into_iter().next() {
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
  D: FnMut((NodeDb, Vec<CargoReplicaTask>)) -> DFut,
  DFut: Future<Output = Result<(), IoError>>,
{
  // The repository transaction commits before this future returns. Only the
  // returned persisted assignments are used to derive exact dispatch tasks.
  let assignments = reconcile(cargo_key.to_owned(), selection.clone()).await?;
  if assignments.len() != selection.total_replicas
    || assignments
      .iter()
      .any(|assignment| assignment.node_name.is_none())
  {
    return Err(IoError::other(
      "CargoScheduling",
      &format!(
        "Cargo {cargo_key:?} has {} assignments for {} desired replicas, including an unassigned replica",
        assignments.len(),
        selection.total_replicas
      ),
    ));
  }
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
  assignment: (NodeDb, Vec<CargoReplicaTask>),
  state: SystemState,
) -> Result<(), IoError> {
  let (node, replicas) = assignment;
  if node.name == state.inner.config.hostname
    || node.name == "init-test.nanocl.io"
  {
    utils::container::cargo::start(&cargo, &replicas, &state).await?;
  } else {
    let client_opts = nanocld_client::ConnectOpts {
      url: node.endpoint,
      ssl: None,
      version: Some(node.version),
    };
    let client = nanocld_client::NanocldClient::connect_to(&client_opts)?;
    let params = nanocl_stubs::node::StartNodeCargoParams {
      cargo_key,
      replicas,
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
      let assignments = reconcile_and_dispatch(
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
      debug_assert_eq!(assignments.len(), cargo.spec.replicas);
      let instances =
        ProcessDb::read_by_kind_key(&cargo_key, None, &state.inner.pool)
          .await?;
      if utils::container::cargo::has_container_healthcheck(&cargo, &instances)
      {
        ObjPsStatusDb::update_health_status(
          &cargo_key,
          &ObjPsHealthStatusKind::Healthy,
          &state.inner.pool,
        )
        .await?;
        state
          .emit_normal_native_action_sync(&cargo, NativeEventAction::Healthy)
          .await;
      }
      ObjPsStatusDb::update_actual_status(
        &cargo_key,
        &ObjPsStatusKind::Start,
        &state.inner.pool,
      )
      .await?;
      state
        .emit_normal_native_action_sync(&cargo, NativeEventAction::Start)
        .await;
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

  use crate::utils::container::generic::SelectionAssignment;
  use crate::vars;

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
  fn assigned_replica_grouping_preserves_stable_identity_and_order() {
    let selection = selection(4, &[("node-b", 1), ("node-a", 2)]);
    // Deliberately use noncontiguous, unsorted node-local ordinals and differ
    // from the planner quotas. Only committed durable assignments may be
    // dispatched.
    let assignments = vec![
      assignment(7, Some("node-a")),
      assignment(1, Some("node-b")),
      assignment(4, Some("node-a")),
      assignment(2, None),
    ];

    let grouped = group_assigned_replicas_by_node(
      "global.scheduler",
      &assignments,
      &selection,
    )
    .unwrap();
    assert_eq!(grouped[0].0.name, "node-b");
    assert_eq!(
      grouped[0].1,
      [CargoReplicaTask {
        key: assignments[1].replica_key,
        ordinal: 1,
      }]
    );
    assert_eq!(grouped[1].0.name, "node-a");
    assert_eq!(
      grouped[1].1,
      [
        CargoReplicaTask {
          key: assignments[2].replica_key,
          ordinal: 4,
        },
        CargoReplicaTask {
          key: assignments[0].replica_key,
          ordinal: 7,
        },
      ]
    );
    assert_eq!(
      grouped
        .iter()
        .map(|(_, replicas)| replicas.len())
        .sum::<usize>(),
      3,
      "the unassigned desired replica must not be dispatched"
    );
  }

  #[ntex::test]
  async fn scheduling_reconciles_before_dispatching_persisted_replicas() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured =
      Arc::new(Mutex::new(Vec::<(String, Vec<(uuid::Uuid, i32)>)>::new()));
    let reconcile_events = events.clone();
    let dispatch_events = events.clone();
    let dispatch_captured = captured.clone();
    let persisted =
      vec![assignment(0, Some("node-a")), assignment(1, Some("node-b"))];
    let returned = persisted.clone();

    let result = reconcile_and_dispatch(
      "global.scheduler",
      selection(2, &[("node-b", 1), ("node-a", 1)]),
      move |cargo_key, selection| {
        let events = reconcile_events.clone();
        let assignments = returned.clone();
        async move {
          assert_eq!(cargo_key, "global.scheduler");
          assert_eq!(selection.total_replicas, 2);
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
      move |(node, replicas)| {
        let events = dispatch_events.clone();
        let captured = dispatch_captured.clone();
        async move {
          events
            .lock()
            .unwrap()
            .push(format!("dispatch:{}", node.name));
          captured.lock().unwrap().push((
            node.name,
            replicas
              .into_iter()
              .map(|replica| (replica.key, replica.ordinal))
              .collect(),
          ));
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
      [
        (
          "node-b".to_owned(),
          vec![(persisted[1].replica_key, persisted[1].ordinal)],
        ),
        (
          "node-a".to_owned(),
          vec![(persisted[0].replica_key, persisted[0].ordinal)],
        ),
      ]
    );
  }

  #[ntex::test]
  async fn incomplete_assignment_prevents_every_dispatch() {
    let dispatches = Arc::new(Mutex::new(0usize));
    let observed_dispatches = dispatches.clone();
    let persisted = vec![
      assignment(0, Some("node-a")),
      assignment(1, Some("node-b")),
      assignment(2, None),
    ];
    let error = reconcile_and_dispatch(
      "global.scheduler",
      selection(3, &[("node-a", 1), ("node-b", 1)]),
      move |_, _| {
        let persisted = persisted.clone();
        async move { Ok(persisted) }
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

    assert_eq!(error.context(), Some("CargoScheduling"));
    assert_eq!(*dispatches.lock().unwrap(), 0);
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
