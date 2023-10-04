use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::secret::Secret;

use super::DisplayFormat;

/// ## SecretCommand
///
/// `nanocl resource` available commands
///
#[derive(Debug, Subcommand)]
pub enum SecretCommand {
  /// Remove existing secret
  #[clap(alias("rm"))]
  Remove(SecretRemoveOpts),
  /// List existing secret
  #[clap(alias("ls"))]
  List,
  /// Inspect a secret
  Inspect(SecretInspectOpts),
}

/// ## SecretArg
///
/// `nanocl secret` available arguments
///
#[derive(Debug, Parser)]
pub struct SecretArg {
  /// Secret command
  #[clap(subcommand)]
  pub command: SecretCommand,
}

/// ## SecretListOpts
///
/// `nanocl secret list` available options
///
#[derive(Debug, Parser)]
pub struct SecretRemoveOpts {
  /// List of secret to remove
  pub keys: Vec<String>,
}

/// ## SecretListOpts
///
/// `nanocl secret inspect` available options
///
#[derive(Debug, Parser)]
pub struct SecretInspectOpts {
  /// Name of secret to inspect
  pub key: String,
  /// Display format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
}

#[derive(Tabled)]
pub struct SecretRow {
  pub key: String,
  pub created_at: String,
  pub updated_at: String,
}

impl From<Secret> for SecretRow {
  fn from(secret: Secret) -> Self {
    // Get the current timezone
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(secret.created_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(secret.updated_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: secret.key,
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}
