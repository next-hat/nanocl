use tabled::Tabled;
use clap::{ValueEnum, Parser, Subcommand};
use serde::{Serialize, Deserialize};

/// Manage nginx templates
#[derive(Debug, Parser)]
pub struct ProxyTemplateArgs {
  #[clap(subcommand)]
  pub(crate) commands: ProxyTemplateCommand,
}

#[derive(Debug, Parser)]
pub struct ProxyTemplateOptions {
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct ProxyArgs {
  #[clap(subcommand)]
  pub commands: ProxyCommands,
}

#[derive(Debug, Parser)]
pub struct ProxyLinkerOptions {
  /// Namespace where the cluster and the template is stored
  #[clap(long)]
  pub(crate) namespace: Option<String>,
  /// Name of cluster
  pub(crate) cl_name: String,
  /// Name of nginx template
  pub(crate) nt_name: String,
}

#[derive(Debug, Subcommand)]
pub enum ProxyCommands {
  /// Manage templates
  Template(ProxyTemplateArgs),
  /// Link a template
  Link(ProxyLinkerOptions),
  /// Unlink a template
  #[clap(alias("rm"))]
  Unlink(ProxyLinkerOptions),
}

#[derive(Debug, Parser)]
pub struct ProxyTemplateCreateOptions {
  /// Name of template to create
  pub(crate) name: String,
  /// Mode of template http|stream
  #[clap(long, short)]
  pub(crate) mode: ProxyTemplateModes,
  /// Create by reading stdi
  #[clap(long = "stdi")]
  pub(crate) is_reading_stdi: bool,
  /// Create by reading a file
  #[clap(short)]
  pub(crate) file_path: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ProxyTemplateCommand {
  /// List existing template
  #[clap(alias("ls"))]
  List,
  /// Create a new template
  Create(ProxyTemplateCreateOptions),
  /// Remove a template
  #[clap(alias("rm"))]
  Remove(ProxyTemplateOptions),
  // Todo
  // Inspect(NginxTemplateOption),
}

/// Nginx template mode
/// # Examples
/// ```
/// NginxTemplateModes::Http; // For http forward
/// NginxTemplateModes::Stream; // For low level tcp/udp forward
/// ```
#[derive(Serialize, Deserialize, Debug, ValueEnum, PartialEq, Eq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ProxyTemplateModes {
  Http,
  Stream,
}

impl std::fmt::Display for ProxyTemplateModes {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ProxyTemplateModes::Http => write!(f, "http"),
      ProxyTemplateModes::Stream => write!(f, "stream"),
    }
  }
}

#[derive(Debug, Tabled, Parser, Serialize, Deserialize)]
pub struct ProxyTemplatePartial {
  /// Name of template to create
  pub(crate) name: String,
  /// Mode of template http|stream
  pub(crate) mode: ProxyTemplateModes,
  /// Content of template
  pub(crate) content: String,
}
