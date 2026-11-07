use bollard_next::models::{ContainerInspectResponse, HealthStatusEnum};
use nanocld_client::stubs::{cargo::CargoInspect, process::ProcessKind};

use crate::models::{
  StateCargoHealth, StateCargoProcessReceipt, StateCargoReceipt,
};

// Runtime identity labels from the daemon's Cargo compiler. Names alone do
// not distinguish application containers from init containers or sandboxes.
const LABEL_ROLE: &str = "io.nanocl.cargo.role";
const LABEL_CONTAINER: &str = "io.nanocl.cargo.container";

fn effective_health(observed: &ContainerInspectResponse) -> StateCargoHealth {
  let Some(config) = observed.config.as_ref() else {
    return StateCargoHealth::Unavailable;
  };
  let test = config
    .healthcheck
    .as_ref()
    .and_then(|healthcheck| healthcheck.test.as_deref());
  if matches!(test, Some([first, ..]) if first.eq_ignore_ascii_case("NONE")) {
    return StateCargoHealth::Disabled;
  }
  // An image-inherited healthcheck can be observed even when the authored
  // Cargo declaration has no Healthcheck. Never infer this from the spec.
  match observed
    .state
    .as_ref()
    .and_then(|state| state.health.as_ref())
    .and_then(|health| health.status)
  {
    Some(HealthStatusEnum::HEALTHY) => StateCargoHealth::Healthy,
    Some(HealthStatusEnum::UNHEALTHY) => StateCargoHealth::Unhealthy,
    Some(HealthStatusEnum::STARTING) => StateCargoHealth::Starting,
    _ if test.is_none_or(|test| test.is_empty()) => StateCargoHealth::Disabled,
    _ => StateCargoHealth::Unavailable,
  }
}

pub(crate) fn cargo_receipt(
  resource: &str,
  inspect: &CargoInspect,
) -> StateCargoReceipt {
  let mut processes = inspect
    .instances
    .iter()
    .filter_map(|process| {
      if process.kind != ProcessKind::Cargo
        || process.kind_key != inspect.spec.cargo_key
        || process.name.ends_with(".tmp.c")
        || process.name.ends_with(".candidate.c")
      {
        return None;
      }
      let labels = process.data.config.as_ref()?.labels.as_ref()?;
      if labels.get(LABEL_ROLE).map(String::as_str) != Some("app") {
        return None;
      }
      let container = labels.get(LABEL_CONTAINER)?;
      if !inspect
        .spec
        .containers
        .iter()
        .any(|declared| declared.name == *container)
      {
        return None;
      }
      Some(StateCargoProcessReceipt {
        name: process.name.clone(),
        node: process.node_name.clone(),
        container: container.clone(),
        running: process.data.state.as_ref().and_then(|state| state.running)
          == Some(true),
        health: effective_health(&process.data),
        image_id: process.data.image.clone().filter(|image| !image.is_empty()),
      })
    })
    .collect::<Vec<_>>();
  processes.sort_by(|a, b| {
    (&a.node, &a.container, &a.name).cmp(&(&b.node, &b.container, &b.name))
  });
  StateCargoReceipt {
    resource: resource.to_owned(),
    revision: inspect.spec.key.to_string(),
    running: processes.iter().filter(|process| process.running).count(),
    total: processes.len(),
    processes,
  }
}

pub(crate) fn format_cargo_receipt(
  receipt: &StateCargoReceipt,
  wide: bool,
) -> String {
  let mut lines = vec![format!(
    "  {}  app processes {}/{} running/observed",
    receipt.resource, receipt.running, receipt.total,
  )];
  if wide {
    lines.push(format!("    revision {}", receipt.revision));
  }
  let prefix = receipt
    .resource
    .strip_prefix("cargo/")
    .map(|key| format!("{key}-"));
  for process in &receipt.processes {
    // Remove only identity already supplied by the parent row. Retain replica
    // ordinals and incarnation suffixes so two observations never look alike.
    let name = if wide {
      process.name.as_str()
    } else {
      prefix
        .as_deref()
        .and_then(|prefix| process.name.strip_prefix(prefix))
        .and_then(|name| name.strip_suffix(".c"))
        .unwrap_or(&process.name)
    };
    lines.push(format!(
      "    {}@{}  {}  health {}",
      name,
      process.node,
      if process.running {
        "running"
      } else {
        "not running"
      },
      process.health,
    ));
    if wide {
      lines.push(format!("      container {}", process.container));
      lines.push(format!(
        "      image ID {}",
        process.image_id.as_deref().unwrap_or("unavailable"),
      ));
    }
  }
  lines.join("\n")
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::{
    cargo_spec::CargoSpecRevision, process::Process,
  };
  use serde_json::json;

  use super::*;

  fn process(name: &str, role: &str) -> Process {
    Process {
      key: format!("id-{name}"),
      created_at: Default::default(),
      updated_at: Default::default(),
      name: name.to_owned(),
      kind: ProcessKind::Cargo,
      node_name: "node-a".to_owned(),
      kind_key: "global.app".to_owned(),
      ip_address: None,
      data: serde_json::from_value(json!({
        "Image": "sha256:local-image-id",
        "Config": {
          "Image": "registry.example/app@sha256:registry-manifest-digest",
          "Labels": {
            LABEL_ROLE: role,
            LABEL_CONTAINER: "api"
          }
        },
        "State": { "Running": true }
      }))
      .unwrap(),
    }
  }

  fn inspect(instances: Vec<Process>) -> CargoInspect {
    CargoInspect {
      namespace_name: "global".to_owned(),
      created_at: Default::default(),
      instance_total: instances.len(),
      instance_running: instances.len(),
      status: Default::default(),
      spec: CargoSpecRevision {
        key: uuid::Uuid::from_u128(42),
        cargo_key: "global.app".to_owned(),
        containers: vec![
          serde_json::from_value(json!({
            "Name": "api", "Image": "registry.example/app:latest"
          }))
          .unwrap(),
        ],
        ..Default::default()
      },
      instances,
    }
  }

  #[test]
  fn cargo_receipt_uses_effective_disabled_and_inherited_health() {
    for (config, state, expected) in [
      (json!({}), json!({}), StateCargoHealth::Disabled),
      (json!(null), json!({}), StateCargoHealth::Unavailable),
      (
        json!({"Healthcheck": {"Test": ["NONE"]}}),
        json!({"Health": {"Status": "healthy"}}),
        StateCargoHealth::Disabled,
      ),
      (
        json!({"Healthcheck": {"Test": ["CMD", "check"]}}),
        json!({}),
        StateCargoHealth::Unavailable,
      ),
      (
        json!({}),
        json!({"Health": {"Status": "healthy"}}),
        StateCargoHealth::Healthy,
      ),
      (
        json!({"Healthcheck": {"Test": ["CMD", "check"]}}),
        json!({"Health": {"Status": "unhealthy"}}),
        StateCargoHealth::Unhealthy,
      ),
      (
        json!({"Healthcheck": {"Test": ["CMD", "check"]}}),
        json!({"Health": {"Status": "starting"}}),
        StateCargoHealth::Starting,
      ),
    ] {
      let observed = serde_json::from_value(json!({
        "Config": config, "State": state
      }))
      .unwrap();
      assert_eq!(effective_health(&observed), expected);
    }
  }

  #[test]
  fn cargo_receipt_counts_only_current_application_processes() {
    let mut stopped = process("global.app-r1.c", "app");
    stopped.data.state.as_mut().unwrap().running = Some(false);
    let mut unrelated = process("global.other-r0.c", "app");
    unrelated.kind_key = "global.other".to_owned();
    let mut unlabelled = process("global.app-r2.c", "app");
    unlabelled.data.config.as_mut().unwrap().labels = None;
    let inspect = inspect(vec![
      process("init-services.app-r0.c", "app"),
      stopped,
      process("global.app-r0.tmp.c", "app"),
      process("global.app-r0.candidate.c", "app"),
      process("global.app-r0.init.c", "init"),
      process("global.app-r0.sandbox.c", "sandbox"),
      unrelated,
      unlabelled,
    ]);
    let receipt = cargo_receipt("cargo/global.app", &inspect);
    assert_eq!(receipt.revision, inspect.spec.key.to_string());
    assert_eq!((receipt.running, receipt.total), (1, 2));
    assert_eq!(receipt.processes.len(), 2);
    assert!(
      receipt
        .processes
        .iter()
        .any(|process| process.name.starts_with("init-services."))
    );
    assert!(
      format_cargo_receipt(&receipt, false)
        .contains("app processes 1/2 running/observed")
    );
  }

  #[test]
  fn cargo_receipt_labels_local_image_id_without_claiming_registry_digest() {
    let receipt = cargo_receipt(
      "cargo/global.app",
      &inspect(vec![process("global.app-r0.c", "app")]),
    );
    let plain = format_cargo_receipt(&receipt, true);
    assert!(plain.contains("image ID sha256:local-image-id"));
    assert!(!plain.contains("manifest"));
    assert!(!plain.contains("digest"));
    let json = serde_json::to_value(&receipt).unwrap();
    assert_eq!(json["processes"][0]["image_id"], "sha256:local-image-id");
    assert!(json["processes"][0].get("image_digest").is_none());
  }

  #[test]
  fn compact_receipt_keeps_processes_distinct_and_wide_retains_full_ids() {
    let mut stopped = process("global.app-r1-api-ghijkl.c", "app");
    stopped.data.state.as_mut().unwrap().running = Some(false);
    let receipt = cargo_receipt(
      "cargo/global.app",
      &inspect(vec![
        process("global.app-r0-api-abcdef.c", "app"),
        stopped,
        // Unexpected names must remain intact rather than losing identity.
        process("other-prefix-r2-api-mnopqr.c", "app"),
      ]),
    );
    let compact = format_cargo_receipt(&receipt, false);
    assert!(compact.contains("app processes 2/3 running/observed"));
    assert!(compact.contains("r0-api-abcdef@node-a  running  health disabled"));
    assert!(
      compact.contains("r1-api-ghijkl@node-a  not running  health disabled")
    );
    assert!(compact.contains("other-prefix-r2-api-mnopqr.c@node-a"));
    assert!(!compact.contains(&receipt.revision));
    assert!(!compact.contains("sha256:local-image-id"));
    assert!(compact.lines().all(|line| line.len() <= 80));

    let wide = format_cargo_receipt(&receipt, true);
    assert!(wide.contains(&format!("revision {}", receipt.revision)));
    assert!(wide.contains("global.app-r0-api-abcdef.c@node-a"));
    assert!(wide.contains("container api"));
    assert!(wide.contains("image ID sha256:local-image-id"));
    assert!(wide.contains("not running  health disabled"));
  }

  #[test]
  fn cargo_receipt_omits_environment_commands_logs_and_arbitrary_metadata() {
    let mut process = process("global.app-r0.c", "app");
    let mut data = serde_json::to_value(&process.data).unwrap();
    data["Config"]["Env"] = json!(["TOKEN=environment-secret"]);
    data["Config"]["Labels"]["private"] = json!("label-secret");
    data["Config"]["Healthcheck"] =
      json!({"Test": ["CMD-SHELL", "command-secret"]});
    data["State"]["Health"] = json!({
      "Status": "healthy", "Log": [{ "Output": "health-log-secret" }]
    });
    process.data = serde_json::from_value(data).unwrap();
    let mut inspect = inspect(vec![process]);
    inspect.spec.metadata = Some(json!({"private": "metadata-secret"}));
    let receipt = cargo_receipt("cargo/global.app", &inspect);
    let json = serde_json::to_string(&receipt).unwrap();
    let plain = format_cargo_receipt(&receipt, false);
    for secret in [
      "environment-secret",
      "label-secret",
      "command-secret",
      "health-log-secret",
      "metadata-secret",
    ] {
      assert!(!json.contains(secret));
      assert!(!plain.contains(secret));
    }
    assert_eq!(receipt.processes[0].health, StateCargoHealth::Healthy);
  }
}
