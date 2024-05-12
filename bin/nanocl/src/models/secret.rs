use serde::Serialize;
use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocl_error::io::IoError;
use nanocld_client::stubs::secret::{Secret, SecretPartial};

use super::{GenericInspectOpts, GenericListOpts, GenericRemoveOpts};

/// `nanocl resource` available commands
#[derive(Clone, Subcommand)]
pub enum SecretCommand {
  /// Remove existing secret
  #[clap(alias("rm"))]
  Remove(GenericRemoveOpts),
  /// List existing secret
  #[clap(alias("ls"))]
  List(GenericListOpts),
  /// Inspect a secret
  Inspect(GenericInspectOpts),
  /// Create a new secret
  Create(SecretCreateOpts),
}

/// `nanocl secret` available arguments
#[derive(Clone, Parser)]
pub struct SecretArg {
  /// Secret command
  #[clap(subcommand)]
  pub command: SecretCommand,
}

/// Create a new nanocl.io/env secret
#[derive(Clone, Parser)]
pub struct EnvCreateOpts {
  /// List of values in the form of `key=value`
  #[clap(required = true)]
  pub values: Vec<String>,
}

/// Create a new nanocl.io/tls secret
#[derive(Clone, Parser, Serialize)]
pub struct TlsCreateOpts {
  /// Certificate
  #[clap(long)]
  pub certificate: String,
  /// Certificate key
  #[clap(long)]
  pub certificate_key: String,
  /// Client certificate
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate_client: Option<String>,
  /// DHParam
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dhparam: Option<String>,
  /// Verify client
  #[clap(long)]
  pub verify_client: bool,
}

/// Create a new nanocl.io/container-registry secret
#[derive(Clone, Parser, Serialize)]
pub struct ContainerRegistryCreateOpts {
  pub username: Option<String>,
  pub password: Option<String>,
  pub auth: Option<String>,
  pub email: Option<String>,
  pub serveraddress: Option<String>,
  pub identitytoken: Option<String>,
  pub registrytoken: Option<String>,
}

impl TryFrom<SecretCreateOpts> for SecretPartial {
  type Error = IoError;
  fn try_from(opts: SecretCreateOpts) -> Result<Self, Self::Error> {
    let (kind, data) = match &opts.kind {
      SecretKindCreateCommand::Env(env) => {
        ("nanocl.io/env", serde_json::to_value(&env.values)?)
      }
      SecretKindCreateCommand::Tls(tls) => {
        ("nanocl.io/tls", serde_json::to_value(tls)?)
      }
      SecretKindCreateCommand::ContainerRegistry(container_registry) => (
        "nanocl.io/container-registry",
        serde_json::to_value(container_registry)?,
      ),
    };
    Ok(Self {
      name: opts.name,
      kind: kind.to_string(),
      immutable: None,
      data,
      metadata: None,
    })
  }
}

#[derive(Clone, Subcommand)]
pub enum SecretKindCreateCommand {
  Env(EnvCreateOpts),
  Tls(TlsCreateOpts),
  ContainerRegistry(ContainerRegistryCreateOpts),
}

/// `nanocl secret create` available options
#[derive(Clone, Parser)]
pub struct SecretCreateOpts {
  /// Name of your secret
  pub name: String,
  /// Kind of secret
  #[clap(subcommand)]
  pub kind: SecretKindCreateCommand,
}

/// A row of the secret table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct SecretRow {
  /// The name of the secret
  pub name: String,
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
      .timestamp_opt(secret.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(secret.updated_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      name: secret.name,
      kind: secret.kind,
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}
