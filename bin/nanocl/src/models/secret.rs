use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::secret::Secret;

use super::DisplayFormat;

/// `nanocl resource` available commands
#[derive(Subcommand)]
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

/// `nanocl secret` available arguments
#[derive(Parser)]
pub struct SecretArg {
  /// Secret command
  #[clap(subcommand)]
  pub command: SecretCommand,
}

/// `nanocl secret list` available options
#[derive(Parser)]
pub struct SecretRemoveOpts {
  /// Skip confirmation
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// List of secret to remove
  pub keys: Vec<String>,
}

/// `nanocl secret inspect` available options
#[derive(Parser)]
pub struct SecretInspectOpts {
  /// Name of secret to inspect
  pub key: String,
  /// Display format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
}

/// A row of the secret table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct SecretRow {
  /// The key of the secret
  pub key: String,
  /// The kind of secret
  pub kind: String,
  /// When the secret have been created
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
  /// When the secret have been updated
  #[tabled(rename = "UPDATED AT")]
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
      kind: secret.kind,
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}
