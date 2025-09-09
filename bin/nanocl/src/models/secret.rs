use chrono::TimeZone;
use clap::{Parser, Subcommand};
use serde::Serialize;
use tabled::Tabled;

use nanocl_error::io::IoError;
use nanocld_client::stubs::secret::{Secret, SecretPartial, SecretUpdate};

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
  /// Update a secret data
  Patch(SecretPatchOpts),
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
#[serde(rename_all = "PascalCase")]
pub struct TlsCreateOpts {
  /// Certificate
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate: Option<String>,
  /// Certificate path to read from a file
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate_path: Option<String>,
  /// Certificate key
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate_key: Option<String>,
  /// Certificate key path to read from a file
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate_key_path: Option<String>,
  /// Client certificate
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate_client: Option<String>,
  /// Client certificate path to read from a file
  #[clap(long)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate_client_path: Option<String>,
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
#[serde(rename_all = "PascalCase")]
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
        let mut cert = tls.certificate.clone();
        let mut cert_key = tls.certificate_key.clone();
        let mut cert_client = tls.certificate_client.clone();
        if cert.is_none() && tls.certificate_path.is_none() {
          return Err(IoError::interrupted("Certificate", "is required"));
        }
        if cert_key.is_none() && tls.certificate_key_path.is_none() {
          return Err(IoError::interrupted("Certificate key", "is required"));
        }
        if let Some(certificate_path) = &tls.certificate_path {
          cert = Some(std::fs::read_to_string(certificate_path)?);
        }
        if let Some(certificate_key_path) = &tls.certificate_key_path {
          cert_key = Some(std::fs::read_to_string(certificate_key_path)?);
        }
        if let Some(certificate_client_path) = &tls.certificate_client_path {
          cert_client = Some(std::fs::read_to_string(certificate_client_path)?);
        }
        let tls = TlsCreateOpts {
          certificate: cert,
          certificate_key: cert_key,
          certificate_client: cert_client,
          certificate_path: None,
          certificate_key_path: None,
          certificate_client_path: None,
          ..tls.clone()
        };
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
      immutable: false,
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

/// Kind for patching secret data
#[derive(Clone, Subcommand)]
pub enum SecretKindPatchCommand {
  /// Update env secret values
  Env(EnvCreateOpts),
  /// Update TLS secret values
  Tls(TlsCreateOpts),
  /// Update container registry secret values
  ContainerRegistry(ContainerRegistryCreateOpts),
}

/// `nanocl secret patch` available options
#[derive(Clone, Parser)]
pub struct SecretPatchOpts {
  /// Name of your secret to update
  pub name: String,
  /// New data of the secret according to its kind
  #[clap(subcommand)]
  pub kind: SecretKindPatchCommand,
}

impl TryFrom<SecretPatchOpts> for SecretUpdate {
  type Error = IoError;

  fn try_from(opts: SecretPatchOpts) -> Result<Self, Self::Error> {
    let data = match &opts.kind {
      SecretKindPatchCommand::Env(env) => serde_json::to_value(&env.values)?,
      SecretKindPatchCommand::Tls(tls) => {
        let mut cert = tls.certificate.clone();
        let mut cert_key = tls.certificate_key.clone();
        let mut cert_client = tls.certificate_client.clone();
        if cert.is_none() && tls.certificate_path.is_none() {
          return Err(IoError::interrupted("Certificate", "is required"));
        }
        if cert_key.is_none() && tls.certificate_key_path.is_none() {
          return Err(IoError::interrupted("Certificate key", "is required"));
        }
        if let Some(certificate_path) = &tls.certificate_path {
          cert = Some(std::fs::read_to_string(certificate_path)?);
        }
        if let Some(certificate_key_path) = &tls.certificate_key_path {
          cert_key = Some(std::fs::read_to_string(certificate_key_path)?);
        }
        if let Some(certificate_client_path) = &tls.certificate_client_path {
          cert_client = Some(std::fs::read_to_string(certificate_client_path)?);
        }
        let tls = TlsCreateOpts {
          certificate: cert,
          certificate_key: cert_key,
          certificate_client: cert_client,
          certificate_path: None,
          certificate_key_path: None,
          certificate_client_path: None,
          ..tls.clone()
        };
        serde_json::to_value(tls)?
      }
      SecretKindPatchCommand::ContainerRegistry(container_registry) => {
        serde_json::to_value(container_registry)?
      }
    };
    Ok(SecretUpdate {
      metadata: None,
      data,
    })
  }
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
