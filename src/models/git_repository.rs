use tabled::Tabled;
use clap::{ValueEnum, Parser, Subcommand};
use serde::{Serialize, Deserialize};

use super::container_image::ProgressDetail;

/// Git repository delete options
#[derive(Debug, Parser)]
pub struct GitRepositoryDeleteOptions {
  /// Name of repository to delete
  pub name: String,
}

/// Git repository build options
#[derive(Debug, Parser)]
pub struct GitRepositoryBuildOptions {
  // Name of git repository to build into container image
  pub name: String,
}

/// Git repository sub commands
#[derive(Debug, Subcommand)]
pub enum GitRepositoryCommands {
  /// List existing git repository
  #[clap(alias("ls"))]
  List,
  /// Create new git repository
  Create(GitRepositoryPartial),
  /// remove git repository
  #[clap(alias("rm"))]
  Remove(GitRepositoryDeleteOptions),
  /// Build a container image from git repository
  Build(GitRepositoryBuildOptions),
}

/// Manage git repositories
#[derive(Debug, Parser)]
pub struct GitRepositoryArgs {
  /// namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: GitRepositoryCommands,
}

#[derive(Clone, Debug, Tabled, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitRepositorySourceType {
  Github,
  Gitlab,
  Local,
}

impl std::fmt::Display for GitRepositorySourceType {
  fn fmt(
    &self,
    f: &mut std::fmt::Formatter<'_>,
  ) -> Result<(), std::fmt::Error> {
    match &self {
      GitRepositorySourceType::Github => write!(f, "github"),
      GitRepositorySourceType::Gitlab => write!(f, "gitlab"),
      GitRepositorySourceType::Local => write!(f, "local"),
    }
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
  #[serde(rename = "code")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub code: Option<i64>,

  #[serde(rename = "message")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}

/// Image ID or Digest
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageId {
  #[serde(rename = "ID")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GithubRepositoryBuildStream {
  #[serde(rename = "id")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,

  #[serde(rename = "stream")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stream: Option<String>,

  #[serde(rename = "error")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,

  #[serde(rename = "errorDetail")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error_detail: Option<ErrorDetail>,

  #[serde(rename = "status")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub status: Option<String>,

  #[serde(rename = "progress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress: Option<String>,

  #[serde(rename = "progressDetail")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress_detail: Option<ProgressDetail>,

  #[serde(rename = "aux")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub aux: Option<ImageId>,
}

#[derive(Tabled, Serialize, Deserialize)]
pub struct GitRepositoryItem {
  pub(crate) name: String,
  pub(crate) url: String,
  pub(crate) default_branch: String,
  pub(crate) source: GitRepositorySourceType,
}

#[derive(Debug, Parser, Serialize)]
pub struct GitRepositoryPartial {
  pub(crate) name: String,
  #[clap(long)]
  pub(crate) url: String,
}
