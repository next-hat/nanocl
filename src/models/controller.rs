use clap::{arg_enum, Parser, Subcommand};
use serde::{Serialize, Deserialize};

#[derive(Debug, Subcommand)]
pub enum ControllerCommands {
  Add(ControllerOptions),
  #[clap(alias("rm"))]
  Remove(ControllerOptions),
}

#[derive(Debug, Parser)]
pub struct ControllerOptions {
  pub(crate) r#type: ControllerType,
}

arg_enum! {
  #[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
  #[serde(rename_all = "snake_case")]
  pub enum ControllerType {
    Dns,
    // GeoDns, // Todo GeoDns and Addition of dnsmasq
    Vpn,
    Proxy,
  }
}

#[derive(Debug, Parser)]
pub struct ControllerArgs {
  #[clap(subcommand)]
  pub(crate) commands: ControllerCommands,
}
