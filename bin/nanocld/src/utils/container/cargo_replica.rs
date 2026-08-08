use std::collections::{BTreeMap, HashSet};

use nanocl_error::io::IoError;

use crate::models::{
  CargoReplicaAssignment, CargoReplicaDb, NewCargoReplicaDb,
};

use super::generic::SelectionPlan;

/// Internal planner or durable-state failure while reconciling Cargo replicas.
#[derive(Debug)]
pub(crate) enum CargoReplicaReconcileError {
  InvalidDesiredReplicaCount {
    cargo_key: String,
    desired_replicas: usize,
  },
  DesiredReplicaCountTooLarge {
    cargo_key: String,
    desired_replicas: usize,
  },
  EmptyPlacementNode {
    cargo_key: String,
  },
  ZeroReplicaAssignment {
    cargo_key: String,
    node_name: String,
  },
  DuplicateNodeAssignment {
    cargo_key: String,
    node_name: String,
  },
  PlacementReplicaCountOverflow {
    cargo_key: String,
  },
  PlacementReplicaCountExceedsDesired {
    cargo_key: String,
    desired_replicas: usize,
    assigned_replicas: usize,
  },
  MissingPlacementNode {
    cargo_key: String,
    node_name: String,
  },
  NonContiguousOrdinal {
    cargo_key: String,
    expected: i32,
    actual: i32,
  },
  ProcessMappingsPreventScaleDown {
    cargo_key: String,
    ordinal: i32,
  },
  ProcessMappingsPreventReassignment {
    cargo_key: String,
    ordinal: i32,
    current_node: Option<String>,
    requested_node: Option<String>,
  },
  IncompleteAssignment {
    cargo_key: String,
    reason: &'static str,
  },
}

impl std::fmt::Display for CargoReplicaReconcileError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::InvalidDesiredReplicaCount {
        cargo_key,
        desired_replicas,
      } => write!(
        f,
        "Cargo {cargo_key:?} has invalid desired replica count {desired_replicas}"
      ),
      Self::DesiredReplicaCountTooLarge {
        cargo_key,
        desired_replicas,
      } => write!(
        f,
        "Cargo {cargo_key:?} desired replica count {desired_replicas} cannot be represented by a replica ordinal"
      ),
      Self::EmptyPlacementNode { cargo_key } => write!(
        f,
        "Cargo {cargo_key:?} placement contains an empty node name"
      ),
      Self::ZeroReplicaAssignment {
        cargo_key,
        node_name,
      } => write!(
        f,
        "Cargo {cargo_key:?} placement contains a zero replica assignment for node {node_name:?}"
      ),
      Self::DuplicateNodeAssignment {
        cargo_key,
        node_name,
      } => write!(
        f,
        "Cargo {cargo_key:?} placement contains duplicate assignments for node {node_name:?}"
      ),
      Self::PlacementReplicaCountOverflow { cargo_key } => write!(
        f,
        "Cargo {cargo_key:?} placement replica count overflows the supported count"
      ),
      Self::PlacementReplicaCountExceedsDesired {
        cargo_key,
        desired_replicas,
        assigned_replicas,
      } => write!(
        f,
        "Cargo {cargo_key:?} placement assigns {assigned_replicas} replicas but only {desired_replicas} are desired"
      ),
      Self::MissingPlacementNode {
        cargo_key,
        node_name,
      } => write!(
        f,
        "Cargo {cargo_key:?} placement references missing node {node_name:?}"
      ),
      Self::NonContiguousOrdinal {
        cargo_key,
        expected,
        actual,
      } => write!(
        f,
        "Cargo {cargo_key:?} has noncontiguous replica ordinal {actual}; expected {expected}"
      ),
      Self::ProcessMappingsPreventScaleDown { cargo_key, ordinal } => write!(
        f,
        "Cargo {cargo_key:?} replica ordinal {ordinal} cannot be removed while it has logical process mappings"
      ),
      Self::ProcessMappingsPreventReassignment {
        cargo_key,
        ordinal,
        current_node,
        requested_node,
      } => write!(
        f,
        "Cargo {cargo_key:?} replica ordinal {ordinal} cannot move from {current_node:?} to {requested_node:?} while it has logical process mappings"
      ),
      Self::IncompleteAssignment { cargo_key, reason } => write!(
        f,
        "Cargo {cargo_key:?} replica reconciliation produced an incomplete assignment: {reason}"
      ),
    }
  }
}

impl std::error::Error for CargoReplicaReconcileError {}

impl From<CargoReplicaReconcileError> for IoError {
  fn from(error: CargoReplicaReconcileError) -> Self {
    IoError::other("CargoReplicaReconcile", error.to_string().as_str())
  }
}

/// Pure durable mutations calculated from the existing placement result.
#[derive(Debug)]
pub(crate) struct CargoReplicaReconcilePlan {
  pub(crate) assignments: Vec<CargoReplicaAssignment>,
  pub(crate) creates: Vec<NewCargoReplicaDb>,
  pub(crate) node_updates: Vec<(uuid::Uuid, i32, Option<String>)>,
  pub(crate) deletes: Vec<CargoReplicaDb>,
}

fn normalized_assignments(
  cargo_key: &str,
  selection: &SelectionPlan,
) -> Result<BTreeMap<String, usize>, CargoReplicaReconcileError> {
  if selection.total_replicas == 0 {
    return Err(CargoReplicaReconcileError::InvalidDesiredReplicaCount {
      cargo_key: cargo_key.to_owned(),
      desired_replicas: selection.total_replicas,
    });
  }
  i32::try_from(selection.total_replicas).map_err(|_| {
    CargoReplicaReconcileError::DesiredReplicaCountTooLarge {
      cargo_key: cargo_key.to_owned(),
      desired_replicas: selection.total_replicas,
    }
  })?;

  let mut normalized = BTreeMap::new();
  let mut assigned_replicas = 0usize;
  for assignment in &selection.assignments {
    let node_name = &assignment.node.name;
    if node_name.is_empty() {
      return Err(CargoReplicaReconcileError::EmptyPlacementNode {
        cargo_key: cargo_key.to_owned(),
      });
    }
    if assignment.replicas == 0 {
      return Err(CargoReplicaReconcileError::ZeroReplicaAssignment {
        cargo_key: cargo_key.to_owned(),
        node_name: node_name.clone(),
      });
    }
    assigned_replicas = assigned_replicas
      .checked_add(assignment.replicas)
      .ok_or_else(|| {
        CargoReplicaReconcileError::PlacementReplicaCountOverflow {
          cargo_key: cargo_key.to_owned(),
        }
      })?;
    if normalized
      .insert(node_name.clone(), assignment.replicas)
      .is_some()
    {
      return Err(CargoReplicaReconcileError::DuplicateNodeAssignment {
        cargo_key: cargo_key.to_owned(),
        node_name: node_name.clone(),
      });
    }
  }
  if assigned_replicas > selection.total_replicas {
    return Err(
      CargoReplicaReconcileError::PlacementReplicaCountExceedsDesired {
        cargo_key: cargo_key.to_owned(),
        desired_replicas: selection.total_replicas,
        assigned_replicas,
      },
    );
  }
  Ok(normalized)
}

pub(crate) fn placement_node_names(
  cargo_key: &str,
  selection: &SelectionPlan,
) -> Result<Vec<String>, CargoReplicaReconcileError> {
  Ok(
    normalized_assignments(cargo_key, selection)?
      .into_keys()
      .collect(),
  )
}

fn take_next_node(
  remaining_assignments: &mut BTreeMap<String, usize>,
) -> Option<String> {
  let (node_name, remaining) = remaining_assignments
    .iter_mut()
    .find(|(_, remaining)| **remaining > 0)?;
  *remaining -= 1;
  Some(node_name.clone())
}

pub(crate) fn plan_reconciliation(
  cargo_key: &str,
  selection: &SelectionPlan,
  existing: &[CargoReplicaDb],
  mapped_replica_keys: &HashSet<uuid::Uuid>,
) -> Result<CargoReplicaReconcilePlan, CargoReplicaReconcileError> {
  let mut remaining_assignments = normalized_assignments(cargo_key, selection)?;
  for (expected, replica) in existing.iter().enumerate() {
    let expected = i32::try_from(expected).map_err(|_| {
      CargoReplicaReconcileError::DesiredReplicaCountTooLarge {
        cargo_key: cargo_key.to_owned(),
        desired_replicas: existing.len(),
      }
    })?;
    if replica.ordinal != expected {
      return Err(CargoReplicaReconcileError::NonContiguousOrdinal {
        cargo_key: cargo_key.to_owned(),
        expected,
        actual: replica.ordinal,
      });
    }
  }

  let retained_count = existing.len().min(selection.total_replicas);
  let retained = &existing[..retained_count];
  let deletes = existing[retained_count..].to_vec();
  if let Some(replica) = deletes
    .iter()
    .rev()
    .find(|replica| mapped_replica_keys.contains(&replica.key))
  {
    return Err(
      CargoReplicaReconcileError::ProcessMappingsPreventScaleDown {
        cargo_key: cargo_key.to_owned(),
        ordinal: replica.ordinal,
      },
    );
  }

  let mut assignments = Vec::with_capacity(selection.total_replicas);
  let mut creates = Vec::new();
  let mut node_updates = Vec::new();
  let mut pending_existing = Vec::new();

  // Reserve every compatible assignment first. A lower null row must not move
  // a compatible higher ordinal merely because it is visited earlier.
  for replica in retained {
    let retained_node = replica.node_name.as_ref().and_then(|node_name| {
      let remaining = remaining_assignments.get_mut(node_name)?;
      if *remaining == 0 {
        return None;
      }
      *remaining -= 1;
      Some(node_name.clone())
    });
    if let Some(node_name) = retained_node {
      assignments.push(CargoReplicaAssignment {
        replica_key: replica.key,
        ordinal: replica.ordinal,
        node_name: Some(node_name),
      });
    } else {
      pending_existing.push(replica);
    }
  }

  // Fill existing gaps and deterministically reassign or clear excess rows.
  for replica in pending_existing {
    let node_name = take_next_node(&mut remaining_assignments);
    if replica.node_name != node_name {
      if mapped_replica_keys.contains(&replica.key) {
        return Err(
          CargoReplicaReconcileError::ProcessMappingsPreventReassignment {
            cargo_key: cargo_key.to_owned(),
            ordinal: replica.ordinal,
            current_node: replica.node_name.clone(),
            requested_node: node_name,
          },
        );
      }
      node_updates.push((replica.key, replica.ordinal, node_name.clone()));
    }
    assignments.push(CargoReplicaAssignment {
      replica_key: replica.key,
      ordinal: replica.ordinal,
      node_name,
    });
  }

  // Scale up only at the highest ordinals. Partial placement deliberately
  // creates durable rows with no node assignment.
  for ordinal in retained_count..selection.total_replicas {
    let ordinal = i32::try_from(ordinal).map_err(|_| {
      CargoReplicaReconcileError::DesiredReplicaCountTooLarge {
        cargo_key: cargo_key.to_owned(),
        desired_replicas: selection.total_replicas,
      }
    })?;
    let node_name = take_next_node(&mut remaining_assignments);
    let mut replica = NewCargoReplicaDb::new(cargo_key, ordinal);
    replica.node_name = node_name.clone();
    assignments.push(CargoReplicaAssignment {
      replica_key: replica.key,
      ordinal,
      node_name,
    });
    creates.push(replica);
  }

  if remaining_assignments
    .values()
    .any(|remaining| *remaining != 0)
  {
    return Err(CargoReplicaReconcileError::IncompleteAssignment {
      cargo_key: cargo_key.to_owned(),
      reason: "node assignments remained after reconciling every desired replica",
    });
  }
  assignments.sort_by_key(|assignment| assignment.ordinal);

  Ok(CargoReplicaReconcilePlan {
    assignments,
    creates,
    node_updates,
    deletes,
  })
}

pub(crate) fn validate_persisted_assignments(
  cargo_key: &str,
  selection: &SelectionPlan,
  replicas: Vec<CargoReplicaDb>,
) -> Result<Vec<CargoReplicaAssignment>, CargoReplicaReconcileError> {
  let expected_counts = normalized_assignments(cargo_key, selection)?;
  if replicas.len() != selection.total_replicas {
    return Err(CargoReplicaReconcileError::IncompleteAssignment {
      cargo_key: cargo_key.to_owned(),
      reason: "persisted row count does not match the desired replica count",
    });
  }

  let mut persisted_counts = BTreeMap::<String, usize>::new();
  let mut assignments = Vec::with_capacity(replicas.len());
  for (expected, replica) in replicas.into_iter().enumerate() {
    let expected = i32::try_from(expected).map_err(|_| {
      CargoReplicaReconcileError::DesiredReplicaCountTooLarge {
        cargo_key: cargo_key.to_owned(),
        desired_replicas: expected,
      }
    })?;
    if replica.ordinal != expected {
      return Err(CargoReplicaReconcileError::NonContiguousOrdinal {
        cargo_key: cargo_key.to_owned(),
        expected,
        actual: replica.ordinal,
      });
    }
    if let Some(node_name) = &replica.node_name {
      *persisted_counts.entry(node_name.clone()).or_default() += 1;
    }
    assignments.push(CargoReplicaAssignment {
      replica_key: replica.key,
      ordinal: replica.ordinal,
      node_name: replica.node_name,
    });
  }
  if persisted_counts != expected_counts {
    return Err(CargoReplicaReconcileError::IncompleteAssignment {
      cargo_key: cargo_key.to_owned(),
      reason: "persisted node counts do not match placement assignments",
    });
  }
  Ok(assignments)
}

#[cfg(test)]
mod tests {
  use crate::{models::NodeDb, vars};

  use super::*;
  use crate::utils::container::generic::{SelectionAssignment, SelectionPlan};

  const CARGO_KEY: &str = "global.reconcile";

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

  fn replica(ordinal: i32, node_name: Option<&str>) -> CargoReplicaDb {
    CargoReplicaDb {
      key: uuid::Uuid::new_v4(),
      cargo_key: CARGO_KEY.to_owned(),
      ordinal,
      node_name: node_name.map(str::to_owned),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
    }
  }

  fn persisted(assignments: &[CargoReplicaAssignment]) -> Vec<CargoReplicaDb> {
    assignments
      .iter()
      .map(|assignment| CargoReplicaDb {
        key: assignment.replica_key,
        cargo_key: CARGO_KEY.to_owned(),
        ordinal: assignment.ordinal,
        node_name: assignment.node_name.clone(),
        created_at: chrono::Utc::now().naive_utc(),
        updated_at: chrono::Utc::now().naive_utc(),
      })
      .collect()
  }

  fn assignment_nodes(
    assignments: &[CargoReplicaAssignment],
  ) -> Vec<(i32, Option<&str>)> {
    assignments
      .iter()
      .map(|assignment| (assignment.ordinal, assignment.node_name.as_deref()))
      .collect()
  }

  fn plan(
    selection: &SelectionPlan,
    existing: &[CargoReplicaDb],
  ) -> Result<CargoReplicaReconcilePlan, CargoReplicaReconcileError> {
    plan_reconciliation(CARGO_KEY, selection, existing, &HashSet::new())
  }

  #[test]
  fn exact_placement_assigns_every_desired_replica() {
    let selection = selection(3, &[("node-b", 1), ("node-a", 2)]);
    let plan = plan(&selection, &[]).unwrap();

    assert_eq!(
      assignment_nodes(&plan.assignments),
      [
        (0, Some("node-a")),
        (1, Some("node-a")),
        (2, Some("node-b")),
      ]
    );
    assert_eq!(plan.creates.len(), 3);
    assert!(plan.node_updates.is_empty());
    assert!(plan.deletes.is_empty());
    assert_eq!(
      validate_persisted_assignments(
        CARGO_KEY,
        &selection,
        persisted(&plan.assignments),
      )
      .unwrap(),
      plan.assignments
    );
  }

  #[test]
  fn partial_placement_persists_full_range_and_is_idempotent() {
    let selection = selection(3, &[("node-b", 1), ("node-a", 1)]);
    let first = plan(&selection, &[]).unwrap();
    assert_eq!(
      assignment_nodes(&first.assignments),
      [(0, Some("node-a")), (1, Some("node-b")), (2, None),]
    );
    assert_eq!(first.creates.len(), 3);

    let rows = persisted(&first.assignments);
    let second = plan(&selection, &rows).unwrap();
    assert_eq!(second.assignments, first.assignments);
    assert!(second.creates.is_empty());
    assert!(second.node_updates.is_empty());
    assert!(second.deletes.is_empty());
    assert_eq!(
      validate_persisted_assignments(CARGO_KEY, &selection, rows).unwrap(),
      first.assignments
    );
  }

  #[test]
  fn newly_available_capacity_assigns_null_replica_without_recreating_it() {
    let partial = selection(3, &[("node-a", 1), ("node-b", 1)]);
    let initial = plan(&partial, &[]).unwrap();
    let rows = persisted(&initial.assignments);
    let null_key = rows[2].key;

    let expanded = selection(3, &[("node-c", 1), ("node-b", 1), ("node-a", 1)]);
    let plan = plan(&expanded, &rows).unwrap();
    assert_eq!(plan.assignments[2].replica_key, null_key);
    assert_eq!(plan.assignments[2].node_name.as_deref(), Some("node-c"));
    assert_eq!(
      plan.node_updates,
      [(null_key, 2, Some("node-c".to_owned()))]
    );
    assert!(plan.creates.is_empty());
    assert!(plan.deletes.is_empty());
  }

  #[test]
  fn capacity_loss_clears_only_excess_assignments() {
    let existing = vec![
      replica(0, Some("node-a")),
      replica(1, Some("node-b")),
      replica(2, Some("node-c")),
    ];
    let selection = selection(3, &[("node-b", 1), ("node-a", 1)]);
    let plan = plan(&selection, &existing).unwrap();

    assert_eq!(
      assignment_nodes(&plan.assignments),
      [(0, Some("node-a")), (1, Some("node-b")), (2, None),]
    );
    assert_eq!(plan.assignments[0].replica_key, existing[0].key);
    assert_eq!(plan.assignments[1].replica_key, existing[1].key);
    assert_eq!(plan.assignments[2].replica_key, existing[2].key);
    assert_eq!(plan.node_updates, [(existing[2].key, 2, None)]);
    assert!(plan.creates.is_empty());
    assert!(plan.deletes.is_empty());
  }

  #[test]
  fn compatible_assignments_are_retained_before_null_gaps_are_filled() {
    let existing = vec![replica(0, None), replica(1, Some("node-a"))];
    let selection = selection(2, &[("node-b", 1), ("node-a", 1)]);
    let plan = plan(&selection, &existing).unwrap();

    assert_eq!(
      assignment_nodes(&plan.assignments),
      [(0, Some("node-b")), (1, Some("node-a"))]
    );
    assert_eq!(
      plan.node_updates,
      [(existing[0].key, 0, Some("node-b".to_owned()))]
    );
  }

  #[test]
  fn scale_up_and_scale_down_preserve_stable_low_ordinals() {
    let existing = vec![replica(0, Some("node-a"))];
    let scaled = selection(3, &[("node-b", 1), ("node-a", 2)]);
    let up = plan(&scaled, &existing).unwrap();
    assert_eq!(up.assignments[0].replica_key, existing[0].key);
    assert_eq!(up.creates.len(), 2);
    assert_eq!(
      up.creates
        .iter()
        .map(|replica| replica.ordinal)
        .collect::<Vec<_>>(),
      [1, 2]
    );

    let rows = persisted(&up.assignments);
    let reduced = selection(1, &[("node-a", 1)]);
    let down = plan(&reduced, &rows).unwrap();
    assert_eq!(down.assignments[0].replica_key, existing[0].key);
    assert_eq!(
      down
        .deletes
        .iter()
        .map(|replica| replica.ordinal)
        .collect::<Vec<_>>(),
      [1, 2]
    );
  }

  #[test]
  fn mapped_scale_down_is_rejected_before_a_plan_is_returned() {
    let existing = vec![
      replica(0, Some("node-a")),
      replica(1, Some("node-a")),
      replica(2, Some("node-a")),
    ];
    let selection = selection(1, &[("node-a", 1)]);
    let mapped = HashSet::from([existing[2].key]);
    let error = plan_reconciliation(CARGO_KEY, &selection, &existing, &mapped)
      .unwrap_err();

    assert!(matches!(
      error,
      CargoReplicaReconcileError::ProcessMappingsPreventScaleDown {
        ordinal: 2,
        ..
      }
    ));
    let error = IoError::from(error);
    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert_eq!(error.context(), Some("CargoReplicaReconcile"));
  }

  #[test]
  fn mapped_replica_reassignment_is_rejected_before_mutation() {
    let existing = vec![replica(0, Some("node-a"))];
    let selection = selection(1, &[("node-b", 1)]);
    let mapped = HashSet::from([existing[0].key]);
    let error = plan_reconciliation(CARGO_KEY, &selection, &existing, &mapped)
      .unwrap_err();

    assert!(matches!(
      error,
      CargoReplicaReconcileError::ProcessMappingsPreventReassignment {
        ordinal: 0,
        current_node: Some(ref current),
        requested_node: Some(ref requested),
        ..
      } if current == "node-a" && requested == "node-b"
    ));
  }

  #[test]
  fn assignment_vector_order_does_not_move_replicas() {
    let existing = vec![replica(0, Some("node-a")), replica(1, Some("node-b"))];
    let first = selection(2, &[("node-b", 1), ("node-a", 1)]);
    let second = selection(2, &[("node-a", 1), ("node-b", 1)]);
    let first = plan(&first, &existing).unwrap();
    let second = plan(&second, &existing).unwrap();

    assert_eq!(first.assignments, second.assignments);
    assert!(first.node_updates.is_empty());
    assert!(second.node_updates.is_empty());
  }

  #[test]
  fn empty_or_underfilled_assignments_are_valid_but_overfill_is_rejected() {
    let empty = selection(2, &[]);
    let empty_plan = plan(&empty, &[]).unwrap();
    assert_eq!(
      assignment_nodes(&empty_plan.assignments),
      [(0, None), (1, None)]
    );

    let underfilled = selection(2, &[("node-a", 1)]);
    assert!(plan(&underfilled, &[]).is_ok());

    let overfilled = selection(2, &[("node-a", 2), ("node-b", 1)]);
    let error = plan(&overfilled, &[]).unwrap_err();
    assert!(matches!(
      error,
      CargoReplicaReconcileError::PlacementReplicaCountExceedsDesired {
        desired_replicas: 2,
        assigned_replicas: 3,
        ..
      }
    ));
    let error = IoError::from(error);
    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert_eq!(error.context(), Some("CargoReplicaReconcile"));
  }

  #[test]
  fn structurally_malformed_assignments_are_rejected() {
    let zero = selection(2, &[("node-a", 0)]);
    assert!(matches!(
      plan(&zero, &[]).unwrap_err(),
      CargoReplicaReconcileError::ZeroReplicaAssignment { .. }
    ));

    let duplicate = selection(2, &[("node-a", 1), ("node-a", 1)]);
    assert!(matches!(
      plan(&duplicate, &[]).unwrap_err(),
      CargoReplicaReconcileError::DuplicateNodeAssignment { .. }
    ));

    let empty_name = selection(1, &[("", 1)]);
    assert!(matches!(
      plan(&empty_name, &[]).unwrap_err(),
      CargoReplicaReconcileError::EmptyPlacementNode { .. }
    ));
  }
}
