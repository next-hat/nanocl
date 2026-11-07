use std::collections::BTreeMap;

use diesel::prelude::*;
use nanocl_error::io::{FromIo, IoResult};
use nanocl_stubs::process::ProcessKind;
use serde_json::{Value, json};

use crate::{
  models::{NetworkDb, ProcessDb, ProcessUpdateDb, ResourceDb, SystemState},
  repositories::{generic::*, operation::run_query_owned},
  schema::{networks, processes, resources},
};

/// Only routing-relevant Docker observations participate in recovery. Health
/// probe logs and observation timestamps change without changing a route.
fn process_route(data: &Value) -> Value {
  json!({
    "Name": data["Name"],
    "NetworkMode": data["HostConfig"]["NetworkMode"],
    "NetworkSettings": data["NetworkSettings"],
    "Running": data["State"]["Running"],
    "Status": data["State"]["Status"],
    "StartedAt": data["State"]["StartedAt"],
    "Health": data["State"]["Health"]["Status"],
  })
}

fn begin_recovery(
  recovered: &mut Option<BTreeMap<String, Value>>,
  observed: &BTreeMap<String, Value>,
  can_reuse: bool,
) -> bool {
  if can_reuse && recovered.as_ref() == Some(observed) {
    return false;
  }
  // Once replay begins, individual controllers may accept different parts of
  // the new observation. Even a return to the old addresses must be replayed
  // after a partial failure.
  *recovered = None;
  true
}

/// Refresh observations only: runtime ownership, desired state, and resource
/// revisions remain owned by their existing mutation paths.
pub(super) async fn recover(
  state: &SystemState,
  recovered: &mut Option<BTreeMap<String, Value>>,
) -> IoResult<()> {
  let node = state.inner.config.hostname.clone();
  // Direct repository queries intentionally inventory all local rows, without
  // the public list API's default 100-item limit.
  let (local_processes, local_networks, rule_keys) =
    run_query_owned(&state.inner.pool, (), move |conn| {
      Ok((
        processes::table
          .filter(processes::node_name.eq(&node))
          .filter(processes::kind.eq_any([
            ProcessKind::Cargo.to_string(),
            ProcessKind::Vm.to_string(),
          ]))
          .load::<ProcessDb>(conn)
          .map_err(ProcessDb::map_err)?,
        networks::table
          .filter(networks::node_name.eq(&node))
          .load::<NetworkDb>(conn)
          .map_err(NetworkDb::map_err)?,
        resources::table
          .filter(resources::kind.eq_any(["ncproxy.io/rule", "ncdns.io/rule"]))
          .order(resources::key.asc())
          .select(resources::key)
          .load::<String>(conn)
          .map_err(ResourceDb::map_err)?,
      ))
    })
    .await?;
  let mut observed = BTreeMap::new();
  let mut complete = true;
  let mut refreshed_routes = false;
  for process in local_processes {
    let instance = match state
      .inner
      .docker_api
      .inspect_container(&process.key, None)
      .await
    {
      Ok(instance) => instance,
      Err(error) => {
        // A missing or temporarily unavailable container is not permission to
        // delete a Process or change workload lifecycle state.
        log::warn!("routing::recover: inspect {}: {error}", process.key);
        complete = false;
        continue;
      }
    };
    let name = instance
      .name
      .as_deref()
      .map(|name| name.trim_start_matches('/'))
      .filter(|name| !name.is_empty())
      .unwrap_or(&process.name)
      .to_owned();
    let data = serde_json::to_value(&instance)
      .map_err(|error| error.map_err_context(|| "Routing observation"))?;
    if data != process.data || name != process.name {
      // An event/startup inventory can have published an older inspection
      // after the previous recovery. Controllers may have consumed it, so a
      // correction must replay even if live Docker matches our cached view.
      refreshed_routes |= name != process.name
        || process_route(&data) != process_route(&process.data);
      if let Err(error) = ProcessDb::update_pk(
        &process.key,
        ProcessUpdateDb {
          name: Some(name),
          data: Some(data.clone()),
          updated_at: Some(chrono::Utc::now().naive_utc()),
          ..Default::default()
        },
        &state.inner.pool,
      )
      .await
      {
        log::warn!("routing::recover: update {}: {error}", process.key);
        complete = false;
        continue;
      }
    }
    observed.insert(format!("process:{}", process.key), process_route(&data));
  }
  for network in local_networks {
    observed.insert(
      format!("network:{}", network.key),
      network.data["IPAM"].clone(),
    );
  }
  if !begin_recovery(recovered, &observed, complete && !refreshed_routes) {
    return Ok(());
  }
  for key in rule_keys {
    if let Err(error) =
      ResourceDb::recover_routing_rule(&key, &state.inner.pool).await
    {
      log::warn!("routing::recover: rule {key}: {error}");
      complete = false;
    }
  }
  // Only a complete pass can suppress retries. This is only in-memory state;
  // every daemon start performs a fresh recovery even if addresses are equal.
  if complete {
    *recovered = Some(observed);
  }
  Ok(())
}

/// Start only after daemon listeners bind: controllers resolve their targets
/// through this daemon while applying recovered rules.
pub fn spawn(state: &SystemState) {
  let state = state.clone();
  ntex::rt::spawn(async move {
    let mut recovered_routes = None;
    let mut last_full_recovery = std::time::Instant::now();
    loop {
      // Proxy callbacks acknowledge generated configuration before the async
      // reload completes, and can temporarily omit unavailable upstreams. A
      // successful snapshot is therefore only a short-lived optimization.
      // Periodic full replay repairs those cases without steady 30s reloads.
      if last_full_recovery.elapsed() >= std::time::Duration::from_secs(300) {
        recovered_routes = None;
        last_full_recovery = std::time::Instant::now();
      }
      if let Err(error) =
        crate::utils::container::network::reconcile_networks(&state).await
      {
        log::warn!("event::network: periodic reconciliation failed: {error}");
      } else if let Err(error) = recover(&state, &mut recovered_routes).await {
        log::warn!("event::routing: recovery failed: {error}");
      }
      ntex::time::sleep(std::time::Duration::from_secs(30)).await;
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn partial_recovery_retries_even_if_addresses_return_to_previous_values() {
    let original = BTreeMap::from([("api".into(), json!("10.0.0.2"))]);
    let changed = BTreeMap::from([("api".into(), json!("10.0.0.3"))]);
    let mut recovered = Some(original.clone());
    assert!(!begin_recovery(&mut recovered, &original, true));
    assert!(begin_recovery(&mut recovered, &changed, true));
    // Some callbacks accepted the changed address, but another failed. The
    // caller does not mark the pass successful before Docker returns to A.
    assert!(begin_recovery(&mut recovered, &original, true));
    assert!(recovered.is_none());
    recovered = Some(original.clone());
    assert!(begin_recovery(&mut recovered, &original, false));
    assert!(recovered.is_none());
  }

  #[test]
  fn routing_observation_detects_ip_changes_and_restarts() {
    let mut instance: bollard_next::models::ContainerInspectResponse = serde_json::from_value(json!({
      "Name": "/global.api.c",
      "HostConfig": {"NetworkMode": "nanoclbr0"},
      "NetworkSettings": {"Networks": {"nanoclbr0": {"IPAddress": "10.0.0.2"}}},
      "State": {"Running": true, "Status": "running", "StartedAt": "first", "Health": {"Status": "healthy"}}
    })).unwrap();
    let initial = serde_json::to_value(&instance).unwrap();
    let before = process_route(&initial);
    let endpoint = instance
      .network_settings
      .as_mut()
      .unwrap()
      .networks
      .as_mut()
      .unwrap()
      .get_mut("nanoclbr0")
      .unwrap();
    assert_eq!(endpoint.ip_address.as_deref(), Some("10.0.0.2"));
    endpoint.ip_address = Some("10.0.0.9".to_owned());
    assert_ne!(
      before,
      process_route(&serde_json::to_value(&instance).unwrap())
    );
    let mut changed = initial.clone();
    changed["State"]["StartedAt"] = json!("after-reboot");
    assert_ne!(before, process_route(&changed));
    changed = initial.clone();
    changed["State"]["Health"]["Status"] = json!("unhealthy");
    assert_ne!(before, process_route(&changed));
    changed = initial;
    changed["State"]["Running"] = json!(false);
    assert_ne!(before, process_route(&changed));
  }

  #[test]
  fn routing_observation_ignores_health_probe_log_churn() {
    let initial = json!({
      "State": {"Running": true, "Health": {"Status": "healthy", "Log": []}}
    });
    let mut next = initial.clone();
    next["State"]["Health"]["Log"] = json!([{"Start": "later", "ExitCode": 0}]);
    assert_eq!(process_route(&initial), process_route(&next));
  }
}
