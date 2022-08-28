use clap::{arg_enum, Parser, Subcommand};
use serde::{Serialize, Deserialize};

#[derive(Debug, Subcommand)]
pub enum NodeCommands {
  Create(NodePartial),
}

#[derive(Debug, Parser)]
pub struct NodeArgs {
  #[clap(subcommand)]
  pub(crate) subcommands: NodeCommands,
}

arg_enum! {
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum SshAuthMode {
    Passwd,
    Rsa,
  }
}

arg_enum! {
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum NodeMode {
    Master,
    Worker,
    Proxy,
  }
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct NodePartial {
  pub(crate) name: String,
  #[clap(long)]
  pub(crate) mode: NodeMode,
  #[clap(long = "ip")]
  pub(crate) ip_address: String,
  #[clap(long = "auth_mode")]
  pub(crate) ssh_auth_mode: SshAuthMode,
  #[clap(long = "user")]
  pub(crate) ssh_user: String,
  #[clap(long = "credential")]
  pub(crate) ssh_credential: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeItem {
  pub(crate) name: String,
  pub(crate) mode: NodeMode,
  pub(crate) ip_address: String,
  pub(crate) ssh_auth_mode: SshAuthMode,
  pub(crate) ssh_user: String,
  pub(crate) ssh_credential: String,
}
