use serde::Serialize;

/// Safe observed fields from a Cargo inspection, without its declaration.
#[derive(Debug, Serialize)]
pub struct StateCargoReceipt {
  pub resource: String,
  pub revision: String,
  /// Counts current observed application processes, not Cargo replicas.
  pub running: usize,
  pub total: usize,
  pub processes: Vec<StateCargoProcessReceipt>,
}

#[derive(Debug, Serialize)]
pub struct StateCargoProcessReceipt {
  pub name: String,
  pub node: String,
  pub container: String,
  pub running: bool,
  pub health: StateCargoHealth,
  /// Docker's local image ID, not a registry manifest digest.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub image_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateCargoHealth {
  Healthy,
  Unhealthy,
  Starting,
  Disabled,
  Unavailable,
}

impl std::fmt::Display for StateCargoHealth {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      Self::Healthy => "healthy",
      Self::Unhealthy => "unhealthy",
      Self::Starting => "starting",
      Self::Disabled => "disabled",
      Self::Unavailable => "unavailable",
    })
  }
}
