use std::{collections::BTreeSet, io::Write};

use nanocl_error::io::IoResult;
use serde_json::Value;

use crate::models::{
  StateDiff, StateDiffAction, StateDiffChange, StateDiffItem, StateDiffSummary,
};

const REDACTED: &str = "[REDACTED]";

/// Compare the authored values before hiding sensitive information. A secret-only
/// change must still be reported even when its displayed values are identical.
#[allow(clippy::too_many_arguments)]
pub(crate) fn diff_item(
  statefile: &str,
  kind: &str,
  name: &str,
  before: Option<&Value>,
  after: Option<&Value>,
  force_update: bool,
  orphan: bool,
  reason: Option<&str>,
) -> StateDiffItem {
  let action = match (before, after) {
    (None, Some(_)) => StateDiffAction::Create,
    (Some(_), None) => StateDiffAction::Remove,
    (Some(before), Some(after)) if force_update || before != after => {
      StateDiffAction::Update
    }
    _ => StateDiffAction::Unchanged,
  };
  let mut changes = Vec::new();
  collect_changes("", kind, before, after, &mut changes);
  StateDiffItem {
    statefile: sanitize_reference(statefile),
    kind: kind.to_owned(),
    name: name.to_owned(),
    action,
    orphan,
    reason: reason.map(str::to_owned),
    changes,
    before: before.and_then(|value| display_config(kind, value)),
    after: after.and_then(|value| display_config(kind, value)),
  }
}

impl StateDiff {
  pub(crate) fn new(items: Vec<StateDiffItem>) -> Self {
    // Keep apply order: a later Statefile may overwrite the same resource.
    let mut summary = StateDiffSummary::default();
    for item in &items {
      match item.action {
        StateDiffAction::Create => summary.created += 1,
        StateDiffAction::Update => summary.updated += 1,
        StateDiffAction::Unchanged => summary.unchanged += 1,
        StateDiffAction::Remove => summary.removed += 1,
      }
      summary.orphans += usize::from(item.orphan);
    }
    Self {
      schema_version: 1,
      items,
      summary,
    }
  }

  pub(crate) fn write_to(
    &self,
    writer: &mut impl Write,
    json: bool,
  ) -> IoResult<()> {
    if json {
      let mut bytes = serde_json::to_vec_pretty(self)?;
      bytes.push(b'\n');
      writer.write_all(&bytes)?;
    } else {
      for item in &self.items {
        super::state_diff_text::write_unified(
          writer,
          &format!("{}/{}", item.kind, item.name),
          item.before.as_ref(),
          item.after.as_ref(),
        )?;
      }
    }
    writer.flush()?;
    Ok(())
  }
}

/// Keep useful image / Statefile references without credentials, signed query
/// parameters or fragments. Docker digest references are left intact.
fn sanitize_reference(value: &str) -> String {
  let end = value.find(['?', '#']).unwrap_or(value.len());
  let mut safe = value[..end].to_owned();
  let authority_start = safe.find("://").map(|i| i + 3).unwrap_or(0);
  let authority_end = safe[authority_start..]
    .find('/')
    .map(|i| authority_start + i)
    .unwrap_or(safe.len());
  if let Some(at) = safe[authority_start..authority_end].rfind('@') {
    let at = authority_start + at;
    // Without a scheme, @sha256:... denotes an image digest, not userinfo.
    if authority_start > 0 || safe[authority_start..at].contains(':') {
      safe.replace_range(authority_start..at, REDACTED);
    }
  }
  if end < value.len() {
    safe.push_str(&value[end..end + 1]);
    safe.push_str(REDACTED);
  }
  safe
}

// These contexts deliberately allow only operational fields. Unknown fields,
// secret Data, environment values, arguments and metadata
// are opaque: neither their values nor their nested keys may reach the report.
fn field_context(context: &str, field: &str) -> &'static str {
  match (context, field) {
    ("resource", "Data") => "resource_data",
    ("resource_data", field) if sensitive_resource_field(field) => "redacted",
    ("resource_data", _) => "resource_data",
    ("namespace" | "secret" | "resource" | "cargo" | "vm" | "job", "Name")
    | ("secret" | "resource", "Kind")
    | ("secret", "Immutable")
    | ("cargo", "Replicas" | "NetworkMode" | "Hostname")
    | ("job", "Schedule" | "Ttl" | "ImagePullPolicy" | "ImagePullSecret")
    | ("vm", "Hostname" | "Image" | "MacAddress") => "scalar",
    ("cargo", "Secrets" | "Dns") | ("job", "Secrets") => "scalars",
    ("cargo", "Containers" | "InitContainers") | ("job", "Containers") => {
      "containers"
    }
    ("vm", "InitContainer") => "container",
    ("cargo", "PortBindings") => "ports",
    ("cargo", "ResourceRequirement") => "requirements",
    ("cargo", "Placement") => "placement",
    ("vm", "HostConfig") => "vm_host",
    (
      "container",
      "Name" | "Essential" | "Image" | "ImagePullPolicy" | "ImagePullSecret"
      | "Hostname" | "Domainname" | "WorkingDir" | "Tty" | "OpenStdin"
      | "StdinOnce" | "StopSignal" | "StopTimeout",
    ) => "scalar",
    ("container", "Secrets") => "scalars",
    ("container", "HostConfig") => "host",
    ("container", "ExposedPorts") => "exposed_ports",
    (
      "host",
      "NetworkMode" | "Memory" | "MemoryReservation" | "MemorySwap"
      | "MemorySwappiness" | "NanoCpus" | "CpuShares" | "CpuPeriod"
      | "CpuQuota" | "CpuRealtimePeriod" | "CpuRealtimeRuntime" | "CpusetCpus"
      | "CpusetMems" | "PidsLimit" | "ReadonlyRootfs" | "Privileged"
      | "AutoRemove" | "ShmSize" | "Init" | "Runtime" | "IpcMode" | "PidMode"
      | "UTSMode" | "PublishAllPorts",
    ) => "scalar",
    ("host", "Binds" | "Dns" | "DnsSearch" | "CapAdd" | "CapDrop") => "scalars",
    ("host", "PortBindings") => "ports",
    ("host", "Mounts") => "mounts",
    ("host", "RestartPolicy") => "restart",
    ("restart", "Name" | "MaximumRetryCount") => "scalar",
    (
      "vm_host",
      "Cpu" | "Memory" | "NetIface" | "LinkNetIface" | "Kvm" | "Runtime"
      | "NetworkMode" | "HostTun",
    ) => "scalar",
    ("vm_host", "Dns") => "scalars",
    (
      "requirements",
      "CpuCores"
      | "CpuUtilizationCap"
      | "MemoryBytes"
      | "MemoryUtilizationCap"
      | "CpuWeight"
      | "MemoryWeight"
      | "StorageBytes",
    ) => "scalar",
    ("placement", "Strategy") => "scalar",
    ("placement", "Regions") => "scalars",
    ("ports", port) if valid_port_key(port) => "bindings",
    ("exposed_ports", port) if valid_port_key(port) => "empty",
    ("binding", "HostIp" | "HostPort") => "scalar",
    ("mount", "Type" | "Source" | "Target" | "ReadOnly" | "Consistency") => {
      "scalar"
    }
    ("mount", "BindOptions") => "bind_options",
    ("mount", "VolumeOptions") => "volume_options",
    ("mount", "TmpfsOptions") => "tmpfs_options",
    (
      "bind_options",
      "Propagation"
      | "NonRecursive"
      | "CreateMountpoint"
      | "ReadOnlyNonRecursive"
      | "ReadOnlyForceRecursive",
    )
    | ("volume_options", "NoCopy" | "Subpath")
    | ("tmpfs_options", "SizeBytes" | "Mode") => "scalar",
    _ => "redacted",
  }
}

/// Resource payloads contain ordinary configuration as well as credential fields.
/// Keep routing/configuration visible instead of hiding the entire Data object.
fn sensitive_resource_field(field: &str) -> bool {
  let key: String = field
    .chars()
    .filter(|c| c.is_ascii_alphanumeric())
    .flat_map(char::to_lowercase)
    .collect();
  key.contains("password")
    || key.contains("passwd")
    || key.contains("secret")
    || key.contains("token")
    || key.ends_with("headers")
    || matches!(
      key.as_str(),
      "authorization"
        | "authentication"
        | "credentials"
        | "credential"
        | "apikey"
        | "privatekey"
        | "clientkey"
        | "signingkey"
        | "encryptionkey"
        | "tlskey"
        | "cert"
        | "certificate"
        | "certificates"
        | "ssl"
        | "tls"
        | "env"
        | "cmd"
        | "command"
        | "entrypoint"
    )
}

fn element_context(context: &str) -> Option<&'static str> {
  match context {
    "scalars" => Some("scalar"),
    "containers" => Some("container"),
    "bindings" => Some("binding"),
    "mounts" => Some("mount"),
    "resource_data" => Some("resource_data"),
    _ => None,
  }
}

/// Build full configurations for YAML context without retaining secret values.
fn display_config(context: &str, value: &Value) -> Option<Value> {
  if context == "redacted" {
    return None;
  }
  if context == "scalar" && (value.is_object() || value.is_array()) {
    return None;
  }
  match value {
    Value::Object(object) => {
      let fields = object
        .iter()
        .filter_map(|(key, value)| {
          display_config(field_context(context, key), value)
            .map(|value| (key.clone(), value))
        })
        .collect();
      Some(Value::Object(fields))
    }
    Value::Array(array) => {
      let context = element_context(context)?;
      Some(Value::Array(
        array
          .iter()
          .filter_map(|value| display_config(context, value))
          .collect(),
      ))
    }
    Value::String(value) => (sanitize_reference(value) == *value)
      .then(|| Value::String(value.clone())),
    _ => Some(value.clone()),
  }
}

fn valid_port_key(value: &str) -> bool {
  let Some((port, protocol)) = value.split_once('/') else {
    return false;
  };
  port.parse::<u16>().is_ok() && matches!(protocol, "tcp" | "udp" | "sctp")
}

fn field_path(path: &str, field: &str) -> String {
  if field
    .chars()
    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
  {
    if path.is_empty() {
      field.to_owned()
    } else {
      format!("{path}.{field}")
    }
  } else {
    // JSON quoting also prevents field names from injecting extra path parts.
    format!("{path}[{}]", Value::String(field.to_owned()))
  }
}

fn collect_changes(
  path: &str,
  context: &str,
  before: Option<&Value>,
  after: Option<&Value>,
  changes: &mut Vec<StateDiffChange>,
) {
  if before == after {
    return;
  }
  if context == "redacted" {
    push_change(path, before, after, true, changes);
    return;
  }
  if context == "scalar"
    || (context == "resource_data"
      && [before, after]
        .into_iter()
        .flatten()
        .all(|value| !value.is_object() && !value.is_array()))
  {
    let structured = [before, after]
      .into_iter()
      .flatten()
      .any(|value| value.is_object() || value.is_array());
    push_change(path, before, after, structured, changes);
    return;
  }
  let element_context = if context == "resource_data"
    && ![before, after].into_iter().flatten().any(Value::is_array)
  {
    None
  } else {
    element_context(context)
  };
  if let Some(element_context) = element_context {
    if [before, after]
      .into_iter()
      .flatten()
      .any(|value| !value.is_array() && !value.is_null())
    {
      push_change(path, before, after, true, changes);
      return;
    }
    let old = before.and_then(Value::as_array);
    let new = after.and_then(Value::as_array);
    let len = old.map_or(0, Vec::len).max(new.map_or(0, Vec::len));
    if len == 0 {
      push_change(path, before, after, false, changes);
    }
    for index in 0..len {
      collect_changes(
        &format!("{path}[{index}]"),
        element_context,
        old.and_then(|values| values.get(index)),
        new.and_then(|values| values.get(index)),
        changes,
      );
    }
    return;
  }
  if [before, after]
    .into_iter()
    .flatten()
    .any(|value| !value.is_object() && !value.is_null())
  {
    push_change(path, before, after, true, changes);
    return;
  }
  let old = before.and_then(Value::as_object);
  let new = after.and_then(Value::as_object);
  let keys: BTreeSet<&String> = old
    .into_iter()
    .flat_map(|object| object.keys())
    .chain(new.into_iter().flat_map(|object| object.keys()))
    .collect();
  if keys.is_empty() {
    push_change(path, before, after, false, changes);
  }
  for key in keys {
    let child_context = field_context(context, key);
    let key_path = if matches!(context, "ports" | "exposed_ports")
      && child_context == "redacted"
    {
      field_path(path, REDACTED)
    } else {
      field_path(path, key)
    };
    collect_changes(
      &key_path,
      child_context,
      old.and_then(|object| object.get(key)),
      new.and_then(|object| object.get(key)),
      changes,
    );
  }
}

fn push_change(
  path: &str,
  before: Option<&Value>,
  after: Option<&Value>,
  redact: bool,
  changes: &mut Vec<StateDiffChange>,
) {
  let safe_value = |value: &Value| {
    if redact {
      Value::String(REDACTED.to_owned())
    } else if let Some(value) = value.as_str() {
      Value::String(sanitize_reference(value))
    } else {
      value.clone()
    }
  };
  let safe_before = before.map(safe_value);
  let safe_after = after.map(safe_value);
  let redacted =
    redact || safe_before.as_ref() != before || safe_after.as_ref() != after;
  changes.push(StateDiffChange {
    path: if path.is_empty() { "$" } else { path }.to_owned(),
    before: safe_before,
    after: safe_after,
    redacted,
  });
}

#[cfg(test)]
mod tests {
  use std::io;

  use serde_json::json;

  use super::*;

  fn item(before: Option<&Value>, after: Option<&Value>) -> StateDiffItem {
    diff_item(
      "Statefile.yml",
      "cargo",
      "web.global",
      before,
      after,
      false,
      false,
      None,
    )
  }

  #[test]
  fn classifies_actions_and_counts_orphans_and_forced_updates() {
    let old = json!({"Replicas": 1});
    let new = json!({"Replicas": 2});
    let created = item(None, Some(&new));
    let updated = item(Some(&old), Some(&new));
    let unchanged = item(Some(&old), Some(&old));
    assert!(unchanged.changes.is_empty());
    let removed = diff_item(
      "Statefile.yml",
      "cargo",
      "orphan.global",
      Some(&old),
      None,
      false,
      true,
      Some("RemoveOrphans is enabled"),
    );
    let forced = diff_item(
      "Statefile.yml",
      "cargo",
      "reload.global",
      Some(&old),
      Some(&old),
      true,
      false,
      Some("reload requested"),
    );
    assert_eq!(forced.action, StateDiffAction::Update);
    assert!(forced.changes.is_empty());
    let report =
      StateDiff::new(vec![created, updated, unchanged, removed, forced]);
    assert_eq!(report.summary.created, 1);
    assert_eq!(report.summary.updated, 2);
    assert_eq!(report.summary.unchanged, 1);
    assert_eq!(report.summary.removed, 1);
    assert_eq!(report.summary.orphans, 1);
  }

  #[test]
  fn highlights_images_ports_mounts_replicas_and_vm_resources() {
    let before = json!({
      "Replicas": 1,
      "PortBindings": {"80/tcp": [{"HostIp": "0.0.0.0", "HostPort": "8080"}]},
      "Containers": [{"Name": "app", "Image": "nginx:1", "HostConfig": {
        "Binds": ["/old:/data:ro"],
        "Mounts": [{"Type": "volume", "Source": "old", "Target": "/cache"}]
      }}]
    });
    let after = json!({
      "Replicas": 3,
      "PortBindings": {"80/tcp": [{"HostIp": "0.0.0.0", "HostPort": "9090"}]},
      "Containers": [{"Name": "app", "Image": "nginx:2", "HostConfig": {
        "Binds": ["/new:/data:ro"],
        "Mounts": [{"Type": "volume", "Source": "new", "Target": "/cache"}]
      }}]
    });
    let changed = item(Some(&before), Some(&after));
    let paths: Vec<_> = changed
      .changes
      .iter()
      .map(|change| change.path.as_str())
      .collect();
    assert_eq!(
      paths,
      [
        "Containers[0].HostConfig.Binds[0]",
        "Containers[0].HostConfig.Mounts[0].Source",
        "Containers[0].Image",
        "PortBindings[\"80/tcp\"][0].HostPort",
        "Replicas",
      ]
    );
    assert!(changed.changes.iter().all(|change| !change.redacted));
    assert_eq!(changed.changes[2].before, Some(json!("nginx:1")));
    assert_eq!(changed.changes[2].after, Some(json!("nginx:2")));
    let before = json!({"HostConfig": {"Cpu": 1, "Memory": 512}});
    let after = json!({"HostConfig": {"Cpu": 2, "Memory": 1024}});
    let changed = diff_item(
      "vm.yml",
      "vm",
      "vm.global",
      Some(&before),
      Some(&after),
      false,
      false,
      None,
    );
    assert_eq!(changed.changes[0].path, "HostConfig.Cpu");
    assert_eq!(changed.changes[1].path, "HostConfig.Memory");
    assert!(changed.changes.iter().all(|change| !change.redacted));
  }

  #[test]
  fn redacts_sensitive_subtrees_when_objects_or_arrays_are_added_or_removed() {
    let config = json!({
      "Metadata": {"hidden-metadata-key": "hidden-metadata-value"},
      "Containers": [{
        "Name": "web", "Image": "nginx:stable",
        "Env": ["TOKEN=hidden-env-value"],
        "Cmd": ["hidden-command-value"],
        "Entrypoint": ["hidden-entrypoint-value"],
        "Labels": {"hidden-label-key": "hidden-label-value"},
        "HostConfig": {"Mounts": [{
          "Type": "volume", "Source": "cache", "Target": "/cache",
          "VolumeOptions": {"DriverConfig": {"Options": {"hidden-option-key": "hidden-option-value"}}}
        }], "UnknownConfig": {"hidden-config-key": "hidden-config-value"}}
      }]
    });
    let empty = json!({});
    for (before, after) in [
      (None, Some(&config)),
      (Some(&config), None),
      (Some(&empty), Some(&config)),
      (Some(&config), Some(&empty)),
    ] {
      let report = StateDiff::new(vec![item(before, after)]);
      for json in [false, true] {
        let mut output = Vec::new();
        report.write_to(&mut output, json).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("hidden-"), "{output}");
        assert_eq!(output.contains(REDACTED), json);
        if !json {
          assert!(!output.contains("Env:"));
          assert!(!output.contains("Metadata"));
          assert!(!output.contains("Cmd:"));
        }
        assert!(output.contains("nginx:stable"));
        assert!(output.contains("cache"));
      }
    }
  }

  #[test]
  fn detects_secret_only_changes_without_exposing_data_or_keys() {
    for kind in ["secret"] {
      let before = json!({"Name": "credentials", "Data": {"hidden-old-key": "hidden-old-value"}});
      let after = json!({"Name": "credentials", "Data": {"hidden-new-key": "hidden-new-value"}});
      for (old, new, action) in [
        (None, Some(&after), StateDiffAction::Create),
        (Some(&before), Some(&after), StateDiffAction::Update),
        (Some(&before), None, StateDiffAction::Remove),
      ] {
        let changed = diff_item(
          "Statefile.yml",
          kind,
          "credentials",
          old,
          new,
          false,
          false,
          None,
        );
        assert_eq!(changed.action, action);
        let data = changed
          .changes
          .iter()
          .find(|change| change.path == "Data")
          .unwrap();
        assert!(data.redacted);
        assert!(data.before.as_ref().is_none_or(|value| value == REDACTED));
        assert!(data.after.as_ref().is_none_or(|value| value == REDACTED));
        assert!(!serde_json::to_string(&changed).unwrap().contains("hidden-"));
      }
    }
    let before = json!({"Containers": [{"Env": ["TOKEN=hidden-before"]}]});
    let after = json!({"Containers": [{"Env": ["TOKEN=hidden-after"]}]});
    let changed = item(Some(&before), Some(&after));
    assert_eq!(changed.action, StateDiffAction::Update);
    assert_eq!(changed.changes.len(), 1);
    assert_eq!(changed.changes[0].path, "Containers[0].Env");
    assert_eq!(changed.changes[0].before, changed.changes[0].after);
    assert!(changed.changes[0].redacted);
  }

  #[test]
  fn sanitizes_reference_credentials_and_escapes_terminal_control_characters() {
    let config = json!({"Containers": [{"Image": "https://hidden-user:hidden-password@registry/image?token=hidden-query#hidden-fragment"}]});
    let changed = diff_item(
      "https://hidden-user:hidden-password@example.com/state.yml?hidden-token",
      "cargo",
      "web\n\u{1b}[31m",
      None,
      Some(&config),
      false,
      false,
      Some("reason\n\u{1b}[32m"),
    );
    assert!(changed.changes[0].redacted);
    let report = StateDiff::new(vec![changed]);
    for json in [false, true] {
      let mut output = Vec::new();
      report.write_to(&mut output, json).unwrap();
      let output = String::from_utf8(output).unwrap();
      assert!(!output.contains("hidden-"), "{output}");
      assert!(!output.contains('\u{1b}'));
      assert_eq!(output.contains("registry/image?"), json);
      assert_eq!(output.contains(REDACTED), json);
      if !json {
        assert!(!output.contains("example.com"));
        assert!(!output.contains("source "));
      }
    }
    assert_eq!(
      sanitize_reference("alpine@sha256:abcd"),
      "alpine@sha256:abcd"
    );
    assert_eq!(
      sanitize_reference("registry/alpine@sha256:abcd"),
      "registry/alpine@sha256:abcd"
    );
    assert_eq!(
      sanitize_reference("user:password@registry/image"),
      "[REDACTED]@registry/image"
    );
  }

  #[test]
  fn renders_deterministic_json_and_human_output() {
    let config = json!({"Replicas": 2});
    let build_report = |reverse: bool| {
      let mut items = vec![
        diff_item(
          "state.yml",
          "cargo",
          "web.global",
          None,
          Some(&config),
          false,
          false,
          None,
        ),
        diff_item(
          "state.yml",
          "cargo",
          "old.global",
          Some(&config),
          None,
          false,
          true,
          None,
        ),
      ];
      if reverse {
        items.reverse();
      }
      StateDiff::new(items)
    };
    assert_eq!(build_report(false).items[0].action, StateDiffAction::Create);
    assert_eq!(build_report(true).items[0].action, StateDiffAction::Remove);
    for json in [false, true] {
      let mut first = Vec::new();
      let mut second = Vec::new();
      build_report(true).write_to(&mut first, json).unwrap();
      build_report(true).write_to(&mut second, json).unwrap();
      assert_eq!(first, second);
      if json {
        let value: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["items"][0]["action"], "remove");
        assert!(value["items"][0].get("reason").is_none());
        assert!(value["items"][0]["changes"][0].get("after").is_none());
        assert_eq!(value["items"][1]["changes"][0]["after"], 2);
      } else {
        let output = String::from_utf8(first).unwrap();
        assert!(output.contains("--- cargo/old.global\n+++ /dev/null\n"));
        assert!(output.contains("--- /dev/null\n+++ cargo/web.global\n"));
        assert!(output.contains("-Replicas: 2\n"));
        assert!(output.contains("+Replicas: 2\n"));
        assert!(!output.contains("diff --git"));
        assert!(!output.contains("created,"));
      }
    }
  }

  #[test]
  fn renders_git_style_updates_without_sensitive_fields() {
    let before = json!({"Replicas": 1, "Containers": [{"Image": "nginx:stable", "Env": ["TOKEN=secret-before"]}]});
    let after = json!({"Replicas": 2, "Containers": [{"Image": "nginx:alpine", "Env": ["TOKEN=secret-after"]}]});
    let report = StateDiff::new(vec![diff_item(
      "Statefile.yml",
      "cargo",
      "global.web",
      Some(&before),
      Some(&after),
      false,
      false,
      None,
    )]);
    let mut output = Vec::new();
    report.write_to(&mut output, false).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.starts_with("--- cargo/global.web\n+++ cargo/global.web\n"));
    assert!(output.contains("-- Image: nginx:stable\n"));
    assert!(output.contains("+- Image: nginx:alpine\n"));
    assert!(output.contains("-Replicas: 1\n"));
    assert!(output.contains("+Replicas: 2\n"));
    for forbidden in [
      "diff --git",
      "action ",
      "source ",
      "created,",
      "Containers[0]",
    ] {
      assert!(!output.contains(forbidden));
    }
    for hidden in [
      "secret-before",
      "secret-after",
      "Env",
      REDACTED,
      "(redacted)",
    ] {
      assert!(!output.contains(hidden));
    }
  }

  #[test]
  fn text_is_silent_for_unchanged_or_hidden_only_differences() {
    let before = json!({"Data": {"password": "secret-before"}});
    let after = json!({"Data": {"password": "secret-after"}});
    let report = StateDiff::new(vec![
      diff_item(
        "Statefile.yml",
        "secret",
        "credentials",
        Some(&before),
        Some(&after),
        false,
        false,
        None,
      ),
      diff_item(
        "Statefile.yml",
        "cargo",
        "global.reload",
        Some(&before),
        Some(&before),
        true,
        false,
        Some("apply --reload forces an update"),
      ),
      diff_item(
        "Statefile.yml",
        "cargo",
        "global.old",
        Some(&before),
        Some(&before),
        false,
        true,
        Some("orphan retained"),
      ),
    ]);
    let mut output = Vec::new();
    report.write_to(&mut output, false).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.is_empty(), "{output}");
    // Machine output still explains non-visible updates and retained orphans.
    let mut json = Vec::new();
    report.write_to(&mut json, true).unwrap();
    let json: Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(json["summary"]["updated"], 2);
    assert_eq!(json["summary"]["unchanged"], 1);
    assert_eq!(json["summary"]["orphans"], 1);
    assert!(json["items"][0].get("before").is_none());
    assert!(json["items"][0].get("after").is_none());
  }

  #[test]
  fn resource_port_change_is_visible_with_yaml_context_and_no_boilerplate() {
    let before = json!({"Name": "deploy-example.com", "Kind": "ncproxy.io/rule", "Data": {
      "Rules": [{"Domain": "deploy-example.com", "Network": "All", "Locations": [{"Path": "/", "Target": {"Key": "global.deploy-example.c", "Port": 9000}}]}]
    }});
    let mut after = before.clone();
    after["Data"]["Rules"][0]["Locations"][0]["Target"]["Port"] = json!(9001);
    let report = StateDiff::new(vec![diff_item(
      "example.yml",
      "resource",
      "deploy-example.com",
      Some(&before),
      Some(&after),
      false,
      false,
      None,
    )]);
    let mut output = Vec::new();
    report.write_to(&mut output, false).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.starts_with(
      "--- resource/deploy-example.com\n+++ resource/deploy-example.com\n"
    ));
    assert!(output.contains("         Key: global.deploy-example.c\n"));
    assert!(
      output.contains("-        Port: 9000\n+        Port: 9001\n"),
      "{output}"
    );
    assert_eq!(
      output,
      concat!(
        "--- resource/deploy-example.com\n+++ resource/deploy-example.com\n",
        "@@ -5,7 +5,7 @@\n",
        "     - Path: /\n",
        "       Target:\n",
        "         Key: global.deploy-example.c\n",
        "-        Port: 9000\n",
        "+        Port: 9001\n",
        "     Network: All\n",
        " Kind: ncproxy.io/rule\n",
        " Name: deploy-example.com\n",
      )
    );
    for forbidden in [
      "git",
      "action update",
      "source ",
      "Sensitive",
      "unchanged",
      "created,",
      REDACTED,
    ] {
      assert!(!output.contains(forbidden), "{output}");
    }
    assert_eq!(report.items[0].changes.len(), 1);
    assert_eq!(report.items[0].changes[0].before, Some(json!(9000)));
    assert_eq!(report.items[0].changes[0].after, Some(json!(9001)));
    assert!(!report.items[0].changes[0].redacted);
  }

  #[test]
  fn resource_credentials_are_hidden_without_hiding_ordinary_data() {
    let before = json!({"Data": {"Port": 9000, "Password": "hidden-old", "Nested": {"ApiToken": "hidden-old-token"}, "Headers": {"Authorization": "hidden-auth"}}});
    let after = json!({"Data": {"Port": 9001, "Password": "hidden-new", "Nested": {"ApiToken": "hidden-new-token"}, "Headers": {"Authorization": "hidden-new-auth"}}});
    for (before, after) in [
      (None, Some(&after)),
      (Some(&before), None),
      (Some(&before), Some(&after)),
    ] {
      let report = StateDiff::new(vec![diff_item(
        "example.yml",
        "resource",
        "example",
        before,
        after,
        false,
        false,
        None,
      )]);
      for json in [false, true] {
        let mut output = Vec::new();
        report.write_to(&mut output, json).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("hidden-"), "{output}");
        assert!(output.contains("Port"), "{output}");
        if !json {
          assert!(!output.contains(REDACTED));
          assert!(!output.contains("Password"));
          assert!(!output.contains("ApiToken"));
        }
      }
    }
  }

  #[test]
  fn propagates_write_and_flush_errors() {
    struct FailingWriter {
      fail_on_flush: bool,
    }
    impl Write for FailingWriter {
      fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.fail_on_flush {
          Ok(bytes.len())
        } else {
          Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
        }
      }
      fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
      }
    }
    let config = json!({"Replicas": 1});
    let report = StateDiff::new(vec![item(None, Some(&config))]);
    for json in [false, true] {
      for fail_on_flush in [false, true] {
        let error = report
          .write_to(&mut FailingWriter { fail_on_flush }, json)
          .unwrap_err();
        assert_eq!(error.inner.kind(), io::ErrorKind::BrokenPipe);
      }
    }
  }
}
