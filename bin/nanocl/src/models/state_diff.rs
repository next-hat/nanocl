use serde::Serialize;
use serde_json::Value;

/// A read-only preview of applying one or more Statefiles.
#[derive(Debug, Serialize)]
pub struct StateDiff {
  pub schema_version: u32,
  pub items: Vec<StateDiffItem>,
  pub summary: StateDiffSummary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateDiffAction {
  Create,
  Update,
  Unchanged,
  Remove,
}

#[derive(Debug, Serialize)]
pub struct StateDiffItem {
  pub statefile: String,
  pub kind: String,
  pub name: String,
  pub action: StateDiffAction,
  pub orphan: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
  pub changes: Vec<StateDiffChange>,
  /// Complete displayable configurations for contextual text diffs only.
  #[serde(skip)]
  pub before: Option<Value>,
  #[serde(skip)]
  pub after: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct StateDiffChange {
  pub path: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub before: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<Value>,
  pub redacted: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct StateDiffSummary {
  pub created: usize,
  pub updated: usize,
  pub unchanged: usize,
  pub removed: usize,
  pub orphans: usize,
}
