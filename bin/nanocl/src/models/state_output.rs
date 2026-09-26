use serde::Serialize;

/// Context shared by records emitted for a state command.
#[derive(Clone)]
pub struct StateOutput {
  pub operation: &'static str,
  pub statefile: Option<String>,
}

#[derive(Serialize)]
pub struct StateOutputRecord<'a> {
  pub schema_version: u8,
  pub operation: &'static str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub statefile: Option<&'a str>,
  #[serde(flatten)]
  pub event: StateOutputEvent<'a>,
}

/// Machine-readable progress and completion records for state commands.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateOutputEvent<'a> {
  State {
    status: &'a str,
    completed: u64,
    total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    success: Option<bool>,
    elapsed_ms: u64,
  },
  Item {
    resource: &'a str,
    status: &'a str,
    completed: u64,
    total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    success: Option<bool>,
    elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
  },
  Image {
    resource: &'a str,
    image: &'a str,
    node: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    layer: Option<&'a str>,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
  },
  Result {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
  },
}
