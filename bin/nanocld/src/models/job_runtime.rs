use std::{
  collections::HashMap,
  sync::{Arc, Mutex, Weak},
};

/// Local serialization only. Durable admission and cancellation live in SQL.
pub(crate) type JobLifecycleLocks =
  Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>;

/// A fresh inspection distinguishes unstarted containers from observed exits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JobRuntimeObservation {
  Running,
  NotStarted,
  Exited(i64),
}

pub(crate) type JobLifecycleLock = Arc<tokio::sync::Mutex<()>>;

/// A cron invocation is pinned to the incarnation that installed its rule.
#[derive(serde::Deserialize)]
pub(crate) struct JobStartQuery {
  pub incarnation: Option<uuid::Uuid>,
}
