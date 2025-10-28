use bollard_next::network::InspectNetworkOptions;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  cargo::Cargo,
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  models::{
    CargoDb, JobDb, JobUpdateDb, MetricDb, NodeDb, ObjPsStatusDb,
    ObjPsStatusUpdate, SystemState, VmDb,
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
pub async fn plan_selection(
  cargo: &Cargo,
  state: &SystemState,
) -> IoResult<Vec<NodeDb>> {
  // Determine desired replicas (default: 1)
  let replicas = cargo
    .spec
    .placement
    .as_ref()
    .and_then(|p| p.replicas)
    .unwrap_or(1)
    .max(1);
  // Extract minimal resource requirements (CPU cores, memory bytes)
  let (cpu_req, mem_req) = cargo
    .spec
    .resource_requirement
    .as_ref()
    .map(|r| (r.cpu, r.memory))
    .unwrap_or((None, None));
  // Extract placement selectors (regions) for require/prefer
  let placement = cargo.spec.placement.as_ref();
  let require_regions: Option<Vec<String>> = placement
    .and_then(|p| p.require.as_ref())
    .and_then(|s| s.regions.clone())
    .or_else(|| placement.and_then(|p| p.regions.clone()));
  let prefer_regions: Option<Vec<String>> = placement
    .and_then(|p| p.prefer.as_ref())
    .and_then(|s| s.regions.clone());
  // Strategy hook (future): we can extend to Balanced/Distinct/UserDefined
  // For now, we select least-loaded nodes that satisfy capacity using SQL.
  // We fetch a small buffer above replicas to allow post-filtering by regions.
  let prelimit = replicas.saturating_mul(5).max(5);
  let mut nodes = MetricDb::select_nodes_for_capacity(
    cpu_req,
    mem_req,
    prelimit,
    &state.inner.pool,
  )
  .await?;
  // Hard filter by required regions if provided (based on node metadata)
  if let Some(req_regions) = &require_regions {
    let req_set: std::collections::HashSet<String> =
      req_regions.iter().cloned().collect();
    nodes.retain(|n| match &n.metadata {
      Some(meta) => {
        let region = meta.get("region").or_else(|| meta.get("Region"));
        match region.and_then(|v| v.as_str()) {
          Some(r) => req_set.contains(r),
          None => false,
        }
      }
      None => false,
    });
  }
  if nodes.is_empty() {
    return Err(IoError::not_found(
      "PlanSelection",
      "No nodes match resource requirements",
    ));
  }
  // Soft preference: bring preferred regions first
  if let Some(pref_regions) = &prefer_regions {
    let pref_set: std::collections::HashSet<String> =
      pref_regions.iter().cloned().collect();
    nodes.sort_by_key(|n| {
      let is_pref = match &n.metadata {
        Some(meta) => {
          let region = meta.get("region").or_else(|| meta.get("Region"));
          match region.and_then(|v| v.as_str()) {
            Some(r) => pref_set.contains(r),
            None => false,
          }
        }
        None => false,
      };
      // Preferred first
      if is_pref { 0 } else { 1 }
    });
  }
  // Truncate to replicas count
  if nodes.len() > replicas {
    nodes.truncate(replicas);
  }
  Ok(nodes)
}
