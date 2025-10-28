use bollard_next::network::InspectNetworkOptions;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  cargo::Cargo,
  cargo_spec::{ConstraintOp, NodeConstraint},
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  models::{
    BalanceWeights, CapacityQuery, CargoDb, JobDb, JobUpdateDb, MetricDb,
    NodeDb, ObjPsStatusDb, ObjPsStatusUpdate, SystemState, VmDb,
  },
  repositories::generic::*,
};

/// Internal utils to emit an event when the state of a process kind changes
/// Eg: (job, cargo, vm)
///
pub async fn emit(
  kind_key: &str,
  kind: &ProcessKind,
  action: NativeEventAction,
  state: &SystemState,
) -> IoResult<()> {
  match kind {
    ProcessKind::Vm => {
      let vm = VmDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
      state.emit_normal_native_action_sync(&vm, action).await;
    }
    ProcessKind::Cargo => {
      let cargo =
        CargoDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
      state.emit_normal_native_action_sync(&cargo, action).await;
    }
    ProcessKind::Job => {
      JobDb::update_pk(
        kind_key,
        JobUpdateDb {
          updated_at: Some(chrono::Utc::now().naive_utc()),
        },
        &state.inner.pool,
      )
      .await?;
      let job =
        JobDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
      state.emit_normal_native_action_sync(&job, action).await;
    }
  }
  Ok(())
}

/// Count the status for the given instances
/// Return a tuple with the total, failed, success and running instances
///
pub fn count_status(instances: &[Process]) -> (usize, usize, usize, usize) {
  let mut instance_failed = 0;
  let mut instance_success = 0;
  let mut instance_running = 0;
  for instance in instances {
    let container = &instance.data;
    let state = container.state.clone().unwrap_or_default();
    if state.restarting.unwrap_or_default() {
      instance_failed += 1;
      continue;
    }
    if state.running.unwrap_or_default() {
      instance_running += 1;
      continue;
    }
    if state.finished_at.unwrap() == "0001-01-01T00:00:00Z" {
      instance_running += 1;
      continue;
    }
    if let Some(exit_code) = state.exit_code {
      if exit_code == 0 {
        instance_success += 1;
      } else {
        instance_failed += 1;
      }
    }
    if let Some(error) = state.error
      && !error.is_empty()
    {
      instance_failed += 1;
    }
  }
  (
    instances.len(),
    instance_failed,
    instance_success,
    instance_running,
  )
}

/// Emit a starting event to the system for the related process object (job, cargo, vm)
/// This will update the status of the process and emit a event
/// So the system start to start the group of processes in the background
///
pub async fn emit_starting(
  kind_key: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> IoResult<()> {
  log::debug!("starting {kind:?} {kind_key}");
  let current_status =
    ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;
  let wanted = if ProcessKind::Job == *kind {
    ObjPsStatusKind::Finish
  } else {
    ObjPsStatusKind::Start
  }
  .to_string();
  let status_update = ObjPsStatusUpdate {
    wanted: Some(wanted),
    prev_wanted: Some(current_status.wanted),
    actual: Some(ObjPsStatusKind::Starting.to_string()),
    prev_actual: Some(current_status.actual),
  };
  ObjPsStatusDb::update_pk(kind_key, status_update, &state.inner.pool).await?;
  emit(kind_key, kind, NativeEventAction::Starting, state).await?;
  Ok(())
}

/// Emit a stopping event to the system for the related process object (job, cargo, vm)
/// This will update the status of the process and emit a event
/// So the system start to stop the group of processes in the background
///
pub async fn emit_stopping(
  kind_key: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> IoResult<()> {
  log::debug!("stopping {kind:?} {kind_key}");
  let current_status =
    ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;
  if current_status.actual == ObjPsStatusKind::Stop.to_string() {
    log::debug!("{kind:?} {kind_key} already stopped",);
    return Ok(());
  }
  let status_update = ObjPsStatusUpdate {
    wanted: Some(ObjPsStatusKind::Stop.to_string()),
    prev_wanted: Some(current_status.wanted),
    actual: Some(ObjPsStatusKind::Stopping.to_string()),
    prev_actual: Some(current_status.actual),
  };
  ObjPsStatusDb::update_pk(kind_key, status_update, &state.inner.pool).await?;
  emit(kind_key, kind, NativeEventAction::Stopping, state).await?;
  Ok(())
}

/// Inject internal data into the payload
/// eg: $$INTERNAL_GATEWAY
///
pub async fn inject_data(
  payload: &str,
  state: &SystemState,
) -> IoResult<String> {
  let network_gateway = state
    .inner
    .docker_api
    .inspect_network("nanoclbr0", None::<InspectNetworkOptions<String>>)
    .await
    .map_err(|err| {
      IoError::interrupted(
        "Network",
        &format!("Unable to inspect network nanoclbr0 {err}"),
      )
    })?;
  let ipam = network_gateway.ipam.unwrap_or_default();
  let ipam_config = ipam.config.unwrap_or_default();
  let Some(network) = ipam_config.first() else {
    return Err(IoError::invalid_data(
      "Network",
      "No network found for nanoclbr0",
    ));
  };
  let gateway_addr = network.gateway.clone().unwrap_or_default();
  let new_data = payload.replace("$$INTERNAL_GATEWAY", &gateway_addr);
  Ok(new_data)
}

/// Plan node selection for a cargo based on placement and resource requirements.
/// This function relies on SQL queries over metrics to avoid loading all nodes in memory.
/// Returns a list of candidate nodes ordered by the chosen strategy (currently least-loaded first).
#[derive(Debug, Clone)]
pub struct SelectionAssignment {
  pub node: NodeDb,
  pub replicas: usize,
}

#[derive(Debug, Clone)]
pub struct SelectionPlan {
  pub total_replicas: usize,
  pub assignments: Vec<SelectionAssignment>,
}

pub async fn plan_selection(
  cargo: &Cargo,
  state: &SystemState,
) -> IoResult<SelectionPlan> {
  // Default metrics recency window (in seconds) to consider a node's metrics as fresh
  const METRIC_RECENCY_DEFAULT_SECS: i64 = 300;
  // Determine desired replicas (default: 1)
  let replicas = cargo
    .spec
    .placement
    .as_ref()
    .and_then(|p| p.replicas)
    .unwrap_or(1)
    .max(1);
  // Extract and normalize minimal resource requirements and caps
  let (cpu_req, mem_req, storage_req, cpu_cap, mem_cap) = cargo
    .spec
    .resource_requirement
    .as_ref()
    .map(|r| {
      (
        r.cpu_cores,
        r.memory_bytes,
        r.storage_bytes,
        r.cpu_utilization_cap,
        r.memory_utilization_cap,
      )
    })
    .unwrap_or((None, None, None, None, None));
  // Validate caps if provided
  if let Some(c) = cpu_cap
    && !(0.0..=1.0).contains(&c)
  {
    return Err(IoError::invalid_data(
      "PlanSelection",
      "cpu_utilization_cap must be between 0.0 and 1.0",
    ));
  }
  if let Some(c) = mem_cap
    && !(0.0..=1.0).contains(&c)
  {
    return Err(IoError::invalid_data(
      "PlanSelection",
      "memory_utilization_cap must be between 0.0 and 1.0",
    ));
  }
  // Extract placement selectors & normalize to NodeConstraints; keep prefer regions for ordering
  let placement = cargo.spec.placement.as_ref();
  let require_selector = placement.and_then(|p| p.require.as_ref());
  let prefer_selector = placement.and_then(|p| p.prefer.as_ref());
  // Build a unified list of required constraints from convenience fields + explicit constraints
  let mut require_constraints: Vec<NodeConstraint> = Vec::new();
  if let Some(sel) = require_selector {
    // Expand convenience arrays into constraints on metadata arrays
    if let Some(labels) = &sel.labels {
      for v in labels {
        require_constraints.push(NodeConstraint {
          key: "labels".into(),
          operator: ConstraintOp::Eq,
          value: v.clone(),
        });
      }
    }
    if let Some(regions) = &sel.regions {
      for v in regions {
        require_constraints.push(NodeConstraint {
          key: "regions".into(),
          operator: ConstraintOp::Eq,
          value: v.clone(),
        });
      }
    }
    if let Some(groups) = &sel.groups {
      for v in groups {
        require_constraints.push(NodeConstraint {
          key: "groups".into(),
          operator: ConstraintOp::Eq,
          value: v.clone(),
        });
      }
    }
    if let Some(extra) = &sel.constraints {
      require_constraints.extend_from_slice(extra);
    }
  }
  // Prefer regions list for soft ordering
  let prefer_regions: Vec<String> = prefer_selector
    .and_then(|s| s.regions.clone())
    .unwrap_or_default();
  // Strategy hook (future): Balanced/Distinct/UserDefined
  // Select nodes via SQL with either least-loaded or balanced strategy.
  // We fetch a buffer above replicas to allow post-filtering by metadata constraints.
  let prelimit = replicas.saturating_mul(10).max(10);
  // Resolve strategy up-front for later logic (eg: Distinct enforcement)
  let strategy = cargo
    .spec
    .placement
    .as_ref()
    .and_then(|p| p.strategy.clone())
    .unwrap_or(nanocl_stubs::cargo_spec::CargoPlacementStrategy::LeastLoaded);
  // Normalize NodeConstraints to SQL-friendly path/value tuples grouped by operator
  let mut req_eq: Vec<(Vec<String>, String)> = Vec::new();
  let mut req_ne: Vec<(Vec<String>, String)> = Vec::new();
  for c in &require_constraints {
    let path: Vec<String> = c.key.split('.').map(|s| s.to_string()).collect();
    match c.operator {
      ConstraintOp::Eq => req_eq.push((path, c.value.clone())),
      ConstraintOp::Ne => req_ne.push((path, c.value.clone())),
    }
  }
  let mut nodes = {
    match strategy {
      nanocl_stubs::cargo_spec::CargoPlacementStrategy::Distinct => {
        // Distinct: prefer at most one replica per node, but degrade gracefully
        // If fewer nodes than replicas, we'll still place extras via round-robin later
        let q = CapacityQuery {
          cpu_cores_required: cpu_req,
          mem_bytes_required: mem_req,
          storage_bytes_required: storage_req,
          cpu_utilization_cap: cpu_cap,
          mem_utilization_cap: mem_cap,
          recency_seconds: Some(METRIC_RECENCY_DEFAULT_SECS),
          limit: prelimit,
          weights: None,
          require_constraints_eq: req_eq.clone(),
          require_constraints_ne: req_ne.clone(),
        };
        MetricDb::select_nodes_for_capacity(&q, &state.inner.pool).await?
      }
      nanocl_stubs::cargo_spec::CargoPlacementStrategy::Balanced => {
        // Derive and normalize weights from spec or defaults
        let (w_cpu, w_mem) = cargo
          .spec
          .resource_requirement
          .as_ref()
          .map(|r| {
            let cpu_w = r.cpu_weight.unwrap_or(0.6);
            let mem_w = r.memory_weight.unwrap_or(0.4);
            let sum = cpu_w + mem_w;
            if sum > 0.0 {
              (cpu_w / sum, mem_w / sum)
            } else {
              (0.5, 0.5)
            }
          })
          .unwrap_or((0.6, 0.4));
        let q = CapacityQuery {
          cpu_cores_required: cpu_req,
          mem_bytes_required: mem_req,
          storage_bytes_required: storage_req,
          cpu_utilization_cap: cpu_cap,
          mem_utilization_cap: mem_cap,
          recency_seconds: Some(METRIC_RECENCY_DEFAULT_SECS),
          limit: prelimit,
          weights: Some(BalanceWeights {
            cpu_weight: w_cpu,
            mem_weight: w_mem,
          }),
          require_constraints_eq: req_eq.clone(),
          require_constraints_ne: req_ne.clone(),
        };
        MetricDb::select_nodes_for_capacity_balanced(&q, &state.inner.pool)
          .await?
      }
      _ => {
        // LeastLoaded and other strategies fallback to CPU-focused ordering
        let q = CapacityQuery {
          cpu_cores_required: cpu_req,
          mem_bytes_required: mem_req,
          storage_bytes_required: storage_req,
          cpu_utilization_cap: cpu_cap,
          mem_utilization_cap: mem_cap,
          recency_seconds: Some(METRIC_RECENCY_DEFAULT_SECS),
          limit: prelimit,
          weights: None,
          require_constraints_eq: req_eq.clone(),
          require_constraints_ne: req_ne.clone(),
        };
        MetricDb::select_nodes_for_capacity(&q, &state.inner.pool).await?
      }
    }
  };
  // Distinct is best-effort: we do not fail if fewer nodes than replicas
  // All filtering is now pushed down to SQL via CapacityQuery constraints
  if nodes.is_empty() {
    return Err(IoError::not_found(
      "PlanSelection",
      "No nodes match resource requirements",
    ));
  }
  // Soft preference: bring preferred regions first
  if !prefer_regions.is_empty() {
    let pref_set: std::collections::HashSet<String> =
      prefer_regions.iter().cloned().collect();
    nodes.sort_by_key(|n| {
      let is_pref = match &n.metadata {
        Some(meta) => {
          // Accept either single string Region/region or array Regions/regions
          let direct = meta.get("region").or_else(|| meta.get("Region"));
          if let Some(r) = direct.and_then(|v| v.as_str()) {
            pref_set.contains(r)
          } else if let Some(arr) = meta
            .get("regions")
            .or_else(|| meta.get("Regions"))
            .and_then(|v| v.as_array())
          {
            arr
              .iter()
              .filter_map(|v| v.as_str())
              .any(|r| pref_set.contains(r))
          } else {
            false
          }
        }
        None => false,
      };
      // Preferred first
      if is_pref { 0 } else { 1 }
    });
  }
  // Truncate to at most replicas nodes (we can place multiple replicas per node)
  if nodes.len() > replicas {
    nodes.truncate(replicas);
  }
  // Compute assignments
  let n = nodes.len();
  if n == 0 {
    return Err(IoError::not_found(
      "PlanSelection",
      "No nodes available after filtering",
    ));
  }
  let assignments = if matches!(
    strategy,
    nanocl_stubs::cargo_spec::CargoPlacementStrategy::Distinct
  ) {
    // Strict distinct: at most one replica per node; if not enough nodes,
    // only schedule up to the number of nodes and let the rest wait.
    nodes
      .into_iter()
      .map(|node| SelectionAssignment { node, replicas: 1 })
      .collect::<Vec<_>>()
  } else {
    // Default: round-robin distribution across selected nodes
    let base = replicas / n;
    let extra = replicas % n;
    let mut acc = Vec::with_capacity(n);
    for (idx, node) in nodes.into_iter().enumerate() {
      let mut count = base;
      if idx < extra {
        count += 1;
      }
      acc.push(SelectionAssignment {
        node,
        replicas: count,
      });
    }
    acc
  };
  Ok(SelectionPlan {
    total_replicas: replicas,
    assignments,
  })
}
