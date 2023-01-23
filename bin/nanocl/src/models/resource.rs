use clap::{Parser, Subcommand};

/// Resource commands
#[derive(Debug, Subcommand)]
pub enum ResourceCommands {
  /// Create new namespace
  // Create(NamespaceOpts),
  /// Inspect a namespace
  // Inspect(NamespaceOpts),
  /// Remove a namespace
  // #[clap(alias("rm"))]
  // Remove(NamespaceOpts),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List,
}

/// Manage resources
#[derive(Debug, Parser)]
#[clap(name = "nanocl-resource")]
pub struct ResourceArgs {
  #[clap(subcommand)]
  pub commands: ResourceCommands,
}
