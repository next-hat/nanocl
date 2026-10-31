use serde::Serialize;

/// Display-safe details of one fully rendered Statefile.
#[derive(Debug, Serialize)]
pub struct StateSummary {
  pub source: String,
  pub namespace: String,
  /// The effective group, with URL credentials and query strings omitted.
  pub group: String,
  pub explicit_group: bool,
  pub counts: StateSummaryCounts,
  pub cargoes: Vec<StateCargoSummary>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct StateSummaryCounts {
  pub cargoes: usize,
  pub virtual_machines: usize,
  pub jobs: usize,
  pub resources: usize,
  pub secrets: usize,
}

#[derive(Debug, Serialize)]
pub struct StateCargoSummary {
  pub name: String,
  pub replicas: usize,
  pub containers: Vec<StateContainerSummary>,
}

#[derive(Debug, Serialize)]
pub struct StateContainerSummary {
  pub name: String,
  pub image: Option<String>,
  pub init: bool,
  pub essential: bool,
  /// Only the policy mode is shown, never the health-check command.
  pub healthcheck: String,
}
