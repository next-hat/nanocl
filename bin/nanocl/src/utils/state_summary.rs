use nanocld_client::stubs::{cargo_spec::ContainerSpec, statefile::Statefile};
use url::Url;

use crate::models::{
  StateCargoSummary, StateContainerSummary, StateRef, StateSummary,
  StateSummaryCounts,
};

/// Summarize a rendered Statefile without retaining its raw text or payloads.
///
/// The caller supplies the effective group from the existing apply logic.
/// Nested Statefiles are summarized separately after they have been rendered.
pub fn summarize_state(
  state: &StateRef<Statefile>,
  effective_group: &str,
) -> StateSummary {
  let data = &state.data;
  StateSummary {
    source: redact_url(&state.location),
    namespace: data.namespace.as_deref().unwrap_or("global").to_owned(),
    group: redact_url(effective_group),
    explicit_group: data.group.is_some(),
    counts: StateSummaryCounts {
      cargoes: data.cargoes.as_ref().map_or(0, Vec::len),
      virtual_machines: data.virtual_machines.as_ref().map_or(0, Vec::len),
      jobs: data.jobs.as_ref().map_or(0, Vec::len),
      resources: data.resources.as_ref().map_or(0, Vec::len),
      secrets: data.secrets.as_ref().map_or(0, Vec::len),
    },
    cargoes: data
      .cargoes
      .iter()
      .flatten()
      .map(|cargo| StateCargoSummary {
        name: cargo.name.clone(),
        replicas: cargo.replicas,
        containers: cargo
          .init_containers
          .iter()
          .map(|container| summarize_container(container, true))
          .chain(
            cargo
              .containers
              .iter()
              .map(|container| summarize_container(container, false)),
          )
          .collect(),
      })
      .collect(),
  }
}

/// Produce a compact plain-text preview from the same safe JSON model.
pub fn format_state_summary(summary: &StateSummary) -> String {
  let counts = &summary.counts;
  let mut lines = vec![
    format!("Statefile {:?}", summary.source),
    format!("  Namespace: {:?}", summary.namespace),
    format!(
      "  Group: {:?} ({})",
      summary.group,
      if summary.explicit_group {
        "explicit"
      } else {
        "derived"
      }
    ),
    format!(
      "  Objects: {} cargoes, {} VMs, {} jobs, {} resources, {} secrets",
      counts.cargoes,
      counts.virtual_machines,
      counts.jobs,
      counts.resources,
      counts.secrets
    ),
  ];
  for cargo in &summary.cargoes {
    lines.push(format!(
      "  Cargo {:?} ({} replicas)",
      cargo.name, cargo.replicas
    ));
    for container in &cargo.containers {
      lines.push(format!(
        "    {}{:?}: image={:?}; health={}; {}",
        if container.init { "init " } else { "" },
        container.name,
        container.image.as_deref().unwrap_or("<not set>"),
        container.healthcheck.replace('_', " "),
        if container.init {
          "must complete"
        } else if container.essential {
          "essential"
        } else {
          "non-essential"
        },
      ));
    }
  }
  lines.join("\n")
}

fn summarize_container(
  container: &ContainerSpec,
  init: bool,
) -> StateContainerSummary {
  let config = &container.container_config;
  let healthcheck = match config
    .healthcheck
    .as_ref()
    .and_then(|health| health.test.as_ref())
    .and_then(|test| test.first())
    .map(String::as_str)
  {
    Some("NONE") => "disabled",
    Some("CMD" | "CMD-SHELL") => "configured",
    _ => "image_default",
  };
  StateContainerSummary {
    name: container.name.clone(),
    image: config.image.as_deref().map(redact_url),
    init,
    essential: container.essential,
    healthcheck: healthcheck.to_owned(),
  }
}

pub(crate) fn redact_url(value: &str) -> String {
  // Local paths and ordinary group names are not URLs. A malformed URL must
  // not fall back to printing a potentially credential-bearing source.
  if !value.contains("://") {
    return value.to_owned();
  }
  let Ok(mut url) = Url::parse(value) else {
    return "<invalid URL>".to_owned();
  };
  let _ = url.set_username("");
  let _ = url.set_password(None);
  url.set_query(None);
  url.set_fragment(None);
  url.to_string()
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::models::{DisplayFormat, StateRoot};

  use super::*;

  fn state(value: serde_json::Value, location: &str) -> StateRef<Statefile> {
    StateRef {
      raw: "raw-content-must-not-appear".to_owned(),
      format: DisplayFormat::Json,
      data: serde_json::from_value(value).expect("valid Statefile fixture"),
      root: StateRoot::None,
      location: location.to_owned(),
    }
  }

  #[test]
  fn defaults_to_global_namespace_and_derived_group() {
    let state = state(json!({"ApiVersion": "v0.18"}), "./Statefile.yml");
    let summary = summarize_state(&state, &state.location);
    assert_eq!(summary.source, "./Statefile.yml");
    assert_eq!(summary.namespace, "global");
    assert_eq!(summary.group, "./Statefile.yml");
    assert!(!summary.explicit_group);
    assert_eq!(summary.counts, StateSummaryCounts::default());
    assert!(summary.cargoes.is_empty());
    assert_eq!(
      format_state_summary(&summary),
      "Statefile \"./Statefile.yml\"\n  Namespace: \"global\"\n  Group: \"./Statefile.yml\" (derived)\n  Objects: 0 cargoes, 0 VMs, 0 jobs, 0 resources, 0 secrets"
    );
  }

  #[test]
  fn counts_only_this_rendered_state_and_preserves_explicit_group() {
    let state = state(
      json!({
        "ApiVersion": "v0.18",
        "Namespace": "production",
        "Group": "release",
        "SubStates": ["./nested.yml"],
        "Cargoes": [
          {"Name": "web", "Replicas": 2, "Containers": [{"Name": "app", "Image": "web:v1"}]},
          {"Name": "store", "Containers": [{"Name": "db", "Image": "postgres:17"}]}
        ],
        "VirtualMachines": [{"Name": "worker", "Image": "/worker.qcow2"}],
        "Jobs": [{"Name": "seed", "Containers": []}],
        "Resources": [{"Name": "route", "Kind": "Proxy", "Data": {}}],
        "Secrets": [
          {"Name": "database", "Kind": "Env", "Data": {}},
          {"Name": "registry", "Kind": "Registry", "Data": {}}
        ]
      }),
      "/deploy/Statefile.yml",
    );
    let summary = summarize_state(&state, "release");
    assert_eq!(summary.namespace, "production");
    assert_eq!(summary.group, "release");
    assert!(summary.explicit_group);
    assert_eq!(
      summary.counts,
      StateSummaryCounts {
        cargoes: 2,
        virtual_machines: 1,
        jobs: 1,
        resources: 1,
        secrets: 2,
      }
    );
    let [web, store] = summary.cargoes.as_slice() else {
      panic!("summary must retain both declared cargoes");
    };
    assert_eq!((web.name.as_str(), store.name.as_str()), ("web", "store"));
    assert_eq!(web.replicas, 2);
    let [app] = web.containers.as_slice() else {
      panic!("web must retain its one declared application container");
    };
    assert_eq!(app.image.as_deref(), Some("web:v1"));
    assert!(!format_state_summary(&summary).contains("nested.yml"));
  }

  #[test]
  fn omits_secret_payloads_commands_and_metadata_from_text_and_json() {
    let state = state(
      json!({
        "ApiVersion": "v0.18",
        "Cargoes": [{
          "Name": "web",
          "Metadata": {"token": "metadata-secret"},
          "Secrets": ["secret-reference"],
          "Containers": [{
            "Name": "app", "Image": "web:v1",
            "Env": ["TOKEN=environment-secret"],
            "Cmd": ["command-secret"],
            "Labels": {"token": "label-secret"},
            "Healthcheck": {"Test": ["CMD-SHELL", "curl -H health-secret localhost"]}
          }]
        }],
        "VirtualMachines": [{"Name": "worker", "Image": "/worker.qcow2", "Password": "vm-password-secret"}],
        "Resources": [{"Name": "route", "Kind": "Proxy", "Data": {"Token": "resource-secret"}}],
        "Secrets": [{"Name": "credentials", "Kind": "Env", "Data": {"TOKEN": "payload-secret"}}]
      }),
      "/deploy/Statefile.yml",
    );
    let summary = summarize_state(&state, &state.location);
    for output in [
      format_state_summary(&summary),
      serde_json::to_string(&summary).expect("serialize summary"),
    ] {
      for secret in [
        "raw-content-must-not-appear",
        "metadata-secret",
        "secret-reference",
        "environment-secret",
        "command-secret",
        "label-secret",
        "health-secret",
        "vm-password-secret",
        "resource-secret",
        "payload-secret",
      ] {
        assert!(!output.contains(secret), "summary exposed {secret}");
      }
      assert!(output.contains("web:v1"));
      assert!(output.contains("configured"));
    }
  }

  #[test]
  fn redacts_url_credentials_queries_and_fragments_in_source_and_group() {
    let source =
      "https://username:password@example.com/deploy.yml?token=secret#private";
    let state = state(json!({"ApiVersion": "v0.18"}), source);
    let summary = summarize_state(&state, source);
    assert_eq!(summary.source, "https://example.com/deploy.yml");
    assert_eq!(summary.group, "https://example.com/deploy.yml");
    assert!(!summary.explicit_group);
    assert_eq!(
      redact_url("https://username:password@[broken?token=secret"),
      "<invalid URL>"
    );
    assert_eq!(redact_url("./config?local.yml"), "./config?local.yml");
  }

  #[test]
  fn reports_health_modes_and_init_completion_without_commands() {
    let state = state(
      json!({
        "ApiVersion": "v0.18",
        "Cargoes": [{
          "Name": "web",
          "InitContainers": [{"Name": "migrate", "Image": "web:v1"}],
          "Containers": [
            {"Name": "default", "Image": "web:v1"},
            {"Name": "disabled", "Image": "web:v1", "Essential": false, "Healthcheck": {"Test": ["NONE"]}},
            {"Name": "configured", "Image": "web:v1", "Healthcheck": {"Test": ["CMD", "private-command"]}},
            {"Name": "inherited", "Image": "web:v1", "Healthcheck": {"Interval": 1000000000}}
          ]
        }]
      }),
      "/deploy/Statefile.yml",
    );
    let summary = summarize_state(&state, &state.location);
    let [web] = summary.cargoes.as_slice() else {
      panic!("summary must retain the declared cargo");
    };
    let [init, default, disabled, configured, inherited] =
      web.containers.as_slice()
    else {
      panic!(
        "summary must retain init and all four application containers in order"
      );
    };
    assert!(init.init);
    assert_eq!(init.name, "migrate");
    assert_eq!(default.healthcheck, "image_default");
    assert_eq!(disabled.healthcheck, "disabled");
    assert!(!disabled.essential);
    assert_eq!(configured.healthcheck, "configured");
    assert_eq!(inherited.healthcheck, "image_default");
    let output = format_state_summary(&summary);
    assert!(output.contains(
      "init \"migrate\": image=\"web:v1\"; health=image default; must complete"
    ));
    assert!(output.contains("health=disabled; non-essential"));
    assert!(!output.contains("private-command"));
  }
}
