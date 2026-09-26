use std::collections::{HashMap, HashSet};

use nanocl_error::io::IoResult;
use nanocl_stubs::process::Process;

use crate::models::{Pool, ProcessDb};

// Bound both database rounds and traversal of stale or circular snapshots.
const MAX_NETWORK_OWNER_DEPTH: usize = 16;

fn network_mode(process: &Process) -> &str {
  process
    .data
    .host_config
    .as_ref()
    .and_then(|config| config.network_mode.as_deref())
    .unwrap_or("nanoclbr0")
}

fn process_index<'a>(
  processes: impl Iterator<Item = &'a Process>,
) -> HashMap<(&'a str, &'a str), &'a Process> {
  let mut index = HashMap::new();
  for process in processes {
    // Prefer exact IDs if a name happens to match another container's ID.
    index
      .entry((process.node_name.as_str(), process.name.as_str()))
      .or_insert(process);
    index.insert((process.node_name.as_str(), process.key.as_str()), process);
  }
  index
}

fn missing_network_owners(
  processes: &[Process],
  owners: &[Process],
  queried: &mut HashSet<(String, String)>,
) -> Vec<(String, String)> {
  let index = process_index(processes.iter().chain(owners));
  processes
    .iter()
    .chain(owners)
    .filter_map(|process| {
      let target = network_mode(process).strip_prefix("container:")?;
      if target.is_empty()
        || index.contains_key(&(process.node_name.as_str(), target))
      {
        return None;
      }
      let target = (process.node_name.clone(), target.to_owned());
      queried.insert(target.clone()).then_some(target)
    })
    .collect()
}

fn effective_ip_address(
  process: &Process,
  index: &HashMap<(&str, &str), &Process>,
) -> Option<String> {
  let mut process = process;
  let mut visited = HashSet::new();
  for _ in 0..=MAX_NETWORK_OWNER_DEPTH {
    if !visited.insert((&process.node_name, &process.key)) {
      return None;
    }
    let mode = network_mode(process);
    if let Some(target) = mode.strip_prefix("container:") {
      process = index.get(&(process.node_name.as_str(), target))?;
      continue;
    }
    if matches!(mode, "host" | "none") {
      return None;
    }
    let settings = process.data.network_settings.as_ref()?;
    let endpoint = settings.networks.as_ref().and_then(|networks| {
      networks.get(mode).or_else(|| {
        networks
          .values()
          .find(|network| network.network_id.as_deref() == Some(mode))
      })
    });
    if let Some(endpoint) = endpoint {
      return endpoint
        .ip_address
        .as_ref()
        .filter(|ip| !ip.is_empty())
        .or_else(|| {
          endpoint
            .global_ipv6_address
            .as_ref()
            .filter(|ip| !ip.is_empty())
        })
        .cloned();
    }
    // Older Docker snapshots may expose bridge addresses only at this level.
    return (mode == "bridge")
      .then(|| settings.ip_address.clone().filter(|ip| !ip.is_empty()))
      .flatten();
  }
  None
}

fn apply_ip_addresses(processes: &mut [Process], owners: &[Process]) {
  let index = process_index(processes.iter().chain(owners));
  let addresses: Vec<_> = processes
    .iter()
    .map(|process| effective_ip_address(process, &index))
    .collect();
  for (process, address) in processes.iter_mut().zip(addresses) {
    process.ip_address = address;
  }
}

/// Enrich list/inspect responses from stored snapshots without changing Data.
pub(super) async fn resolve_ip_addresses(
  processes: &mut [Process],
  pool: &Pool,
) -> IoResult<()> {
  let mut owners = Vec::new();
  let mut queried = HashSet::new();
  for _ in 0..MAX_NETWORK_OWNER_DEPTH {
    let targets = missing_network_owners(processes, &owners, &mut queried);
    if targets.is_empty() {
      break;
    }
    owners.extend(ProcessDb::read_network_owners(&targets, pool).await?);
  }
  apply_ip_addresses(processes, &owners);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn process(key: &str, node: &str, mode: &str) -> Process {
    serde_json::from_value(json!({
      "Key": key,
      "Name": format!("{key}.c"),
      "Kind": "cargo",
      "KindKey": "global.app",
      "NodeName": node,
      "CreatedAt": "2026-09-26T00:00:00",
      "UpdatedAt": "2026-09-26T00:00:00",
      "Data": {
        "Id": key,
        "HostConfig": {"NetworkMode": mode},
        "NetworkSettings": {"Networks": {
          "app-net": {"IPAddress": "10.42.0.7", "NetworkID": "network-id"},
          "other-net": {"IPAddress": "10.99.0.9"}
        }}
      }
    }))
    .unwrap()
  }

  #[test]
  fn shared_ip_uses_owner_outside_page_and_preserves_inspect_data() {
    let mut processes = vec![process("app", "node-a", "container:owner")];
    let before = serde_json::to_value(&processes[0].data).unwrap();
    let owner = process("owner", "node-a", "app-net");
    let mut queried = HashSet::new();
    assert_eq!(
      missing_network_owners(&processes, &[], &mut queried),
      vec![("node-a".to_owned(), "owner".to_owned())]
    );
    apply_ip_addresses(&mut processes, &[owner]);
    assert_eq!(processes[0].ip_address.as_deref(), Some("10.42.0.7"));
    assert_eq!(serde_json::to_value(&processes[0].data).unwrap(), before);
  }

  #[test]
  fn owner_lookups_are_deduplicated_and_scoped_to_the_process_node() {
    let mut processes = vec![
      process("app", "node-a", "container:owner"),
      process("sidecar", "node-a", "container:owner"),
      process("remote", "node-b", "container:owner"),
    ];
    let owners = vec![process("owner", "node-b", "app-net")];
    let mut queried = HashSet::new();
    assert_eq!(
      missing_network_owners(&processes, &owners, &mut queried),
      vec![("node-a".to_owned(), "owner".to_owned())]
    );
    assert!(
      missing_network_owners(&processes, &owners, &mut queried).is_empty()
    );
    apply_ip_addresses(&mut processes, &owners);
    assert_eq!(processes[0].ip_address, None);
    assert_eq!(processes[1].ip_address, None);
    assert_eq!(processes[2].ip_address.as_deref(), Some("10.42.0.7"));
  }

  #[test]
  fn network_selection_handles_named_networks_bridge_ipv6_and_no_address() {
    let mut processes = vec![
      process("named", "node", "app-net"),
      process("network-id", "node", "network-id"),
      process("host", "node", "host"),
      process("none", "node", "none"),
      process("missing", "node", "missing-net"),
      process("ipv6", "node", "app-net"),
      process("empty", "node", "app-net"),
      process("bridge", "node", "bridge"),
    ];
    for (index, ipv6) in [(5, "fd00::7"), (6, "")] {
      let network = processes[index]
        .data
        .network_settings
        .as_mut()
        .unwrap()
        .networks
        .as_mut()
        .unwrap()
        .get_mut("app-net")
        .unwrap();
      network.ip_address = Some(String::new());
      network.global_ipv6_address = Some(ipv6.to_owned());
    }
    let bridge = processes[7].data.network_settings.as_mut().unwrap();
    bridge.networks = None;
    bridge.ip_address = Some("172.17.0.2".to_owned());
    apply_ip_addresses(&mut processes, &[]);
    let addresses: Vec<_> =
      processes.iter().map(|p| p.ip_address.as_deref()).collect();
    assert_eq!(
      addresses,
      vec![
        Some("10.42.0.7"),
        Some("10.42.0.7"),
        None,
        None,
        None,
        Some("fd00::7"),
        None,
        Some("172.17.0.2")
      ]
    );
  }

  #[test]
  fn shared_network_chains_resolve_names_and_stop_at_cycles_or_missing_owners()
  {
    let mut processes = vec![
      process("app", "node", "container:middle.c"),
      process("missing", "node", "container:absent"),
      process("cycle", "node", "container:cycle"),
      process("empty", "node", "container:"),
    ];
    let owners = vec![
      process("middle", "node", "container:owner"),
      process("owner", "node", "app-net"),
    ];
    apply_ip_addresses(&mut processes, &owners);
    assert_eq!(processes[0].ip_address.as_deref(), Some("10.42.0.7"));
    assert!(processes[1..].iter().all(|p| p.ip_address.is_none()));

    let owners: Vec<_> = (0..=MAX_NETWORK_OWNER_DEPTH)
      .map(|i| {
        process(
          &format!("owner-{i}"),
          "node",
          &format!("container:owner-{}", i + 1),
        )
      })
      .chain([process(
        &format!("owner-{}", MAX_NETWORK_OWNER_DEPTH + 1),
        "node",
        "app-net",
      )])
      .collect();
    let mut processes = vec![process("app", "node", "container:owner-0")];
    apply_ip_addresses(&mut processes, &owners);
    assert_eq!(processes[0].ip_address, None);
  }
}
