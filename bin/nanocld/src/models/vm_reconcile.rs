use clap::{Args, Subcommand};

/// Offline maintenance commands do not start the daemon or its task runners.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum DaemonCommand {
  /// Inspect or reconcile one interrupted VM while all owners are quiescent
  VmReconcile(VmReconcileArgs),
}

#[derive(Debug, Clone, Args)]
pub(crate) struct VmReconcileArgs {
  /// VM key, including its namespace
  #[arg(long)]
  pub vm_key: String,
  /// Exact incarnation UUID printed by the inspection command
  #[arg(long)]
  pub incarnation: Option<uuid::Uuid>,
  /// Exact current operation UUID printed by the inspection command
  #[arg(long)]
  pub operation: Option<uuid::Uuid>,
  /// Exact current specification UUID printed by the inspection command
  #[arg(long)]
  pub spec: Option<uuid::Uuid>,
  /// Confirm every daemon/owner using this database and runtime is stopped,
  /// including daemons using another state directory
  #[arg(long)]
  pub confirm_quiescent: bool,
}
