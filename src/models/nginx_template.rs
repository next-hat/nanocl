use tabled::Tabled;
use clap::{ValueEnum, Parser, Subcommand};
use serde::{Serialize, Deserialize};

/// Manage nginx templates
#[derive(Debug, Parser)]
pub struct NginxTemplateArgs {
  #[clap(subcommand)]
  pub(crate) commands: NginxTemplateCommand,
}

#[derive(Debug, Parser)]
pub struct NginxTemplateOptions {
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct NginxTemplateCreateOptions {
  /// Name of template to create
  pub(crate) name: String,
  /// Mode of template http|stream
  #[clap(long, short)]
  pub(crate) mode: NginxTemplateModes,
  /// Create by reading stdi
  #[clap(long = "stdi")]
  pub(crate) is_reading_stdi: bool,
  /// Create by reading a file
  #[clap(short)]
  pub(crate) file_path: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum NginxTemplateCommand {
  /// List existing template
  #[clap(alias("ls"))]
  List,
  /// Create a new template
  Create(NginxTemplateCreateOptions),
  /// Remove a template
  #[clap(alias("rm"))]
  Remove(NginxTemplateOptions),
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
pub enum NginxTemplateModes {
  Http,
  Stream,
}

impl std::fmt::Display for NginxTemplateModes {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      NginxTemplateModes::Http => write!(f, "http"),
      NginxTemplateModes::Stream => write!(f, "stream"),
    }
  }
}

#[derive(Debug, Tabled, Parser, Serialize, Deserialize)]
pub struct NginxTemplatePartial {
  /// Name of template to create
  pub(crate) name: String,
  /// Mode of template http|stream
  pub(crate) mode: NginxTemplateModes,
  /// Content of template
  pub(crate) content: String,
}
