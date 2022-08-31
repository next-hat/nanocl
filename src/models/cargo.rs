use clap::{Parser, Subcommand, AppSettings};
use tabled::Tabled;
use serde::{Serialize, Deserialize};

use super::container::ContainerSummary;
use super::utils::tabled::*;

/// Cargo delete options
#[derive(Debug, Parser)]
pub struct CargoDeleteOptions {
  /// Name of cargo to delete
  pub name: String,
}

/// Cargo start options
#[derive(Debug, Parser)]
pub struct CargoStartOptions {
  // Name of cargo to start
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct CargoInspectOption {
  /// Name of cargo to inspect
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum CargoPatchCommands {
  Set(CargoPatchPartial),
}

#[derive(Debug, Parser)]
pub struct CargoPatchArgs {
  pub(crate) name: String,
  #[clap(subcommand)]
  pub(crate) commands: CargoPatchCommands,
}

#[derive(Debug, Subcommand)]
#[clap(
  about,
  version,
  global_setting = AppSettings::DeriveDisplayOrder,
)]
pub enum CargoCommands {
  /// List existing cargo
  #[clap(alias("ls"))]
  List,
  /// Create a new cargo
  Create(CargoPartial),
  /// Remove cargo by it's name
  #[clap(alias("rm"))]
  Remove(CargoDeleteOptions),
  /// Inspect a cargo by it's name
  Inspect(CargoInspectOption),
  /// Update a cargo by it's name
  Patch(CargoPatchArgs),
}

/// Manage cargoes
#[derive(Debug, Parser)]
#[clap(name = "nanocl-cargo")]
pub struct CargoArgs {
  /// namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: CargoCommands,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct CargoPartial {
  /// Name of the cargo
  pub(crate) name: String,
  /// name of the image
  #[clap(long = "image")]
  pub(crate) image_name: String,
  /// Optional domain to bind to in format ip:domain.com
  #[clap(long)]
  pub(crate) dns_entry: Option<String>,
  #[clap(long)]
  pub(crate) domainname: Option<String>,
  #[clap(long)]
  pub(crate) hostname: Option<String>,
  /// proxy config is an optional string as follow domain_name=your_domain,host_ip=your_host_ip
  // #[clap(long)]
  // pub(crate) proxy_config: Option<CargoProxyConfigPartial>,
  #[clap(long = "-bind")]
  /// Directory or volumes to create
  pub(crate) binds: Option<Vec<String>>,
  /// Environement variable
  #[clap(long = "-env")]
  pub(crate) environnements: Option<Vec<String>>,
  /// Number of replicas default to 1
  #[clap(long)]
  pub(crate) replicas: Option<i32>,
}

/// Cargo item is an definition to container create image and start them
/// this structure ensure read and write in database
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct CargoItem {
  pub(crate) key: String,
  pub(crate) name: String,
  #[serde(rename = "image_name")]
  pub(crate) image: String,
  pub(crate) replicas: i32,
  // #[serde(rename = "network_name")]
  // pub(crate) network: Option<String>,
  #[serde(rename = "namespace_name")]
  pub(crate) namespace: String,
}

/// Cargo item with his relation
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct CargoItemWithRelation {
  pub(crate) key: String,
  #[tabled(skip)]
  pub(crate) namespace_name: String,
  pub(crate) name: String,
  pub(crate) image_name: String,
  pub(crate) replicas: i32,
  #[tabled(display_with = "optional_string")]
  pub(crate) domainname: Option<String>,
  #[tabled(display_with = "optional_string")]
  pub(crate) hostname: Option<String>,
  #[tabled(display_with = "optional_string")]
  pub(crate) dns_entry: Option<String>,
  #[tabled(skip)]
  pub(crate) binds: Vec<String>,
  #[tabled(skip)]
  pub(crate) containers: Vec<ContainerSummary>,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct CargoPatchPartial {
  #[clap(long)]
  pub(crate) name: Option<String>,
  #[clap(long = "image")]
  pub(crate) image_name: Option<String>,
  #[clap(long = "bind")]
  pub(crate) binds: Option<Vec<String>>,
  #[clap(long)]
  pub(crate) replicas: Option<i32>,
  #[clap(long)]
  pub(crate) dns_entry: Option<String>,
  #[clap(long)]
  pub(crate) domainname: Option<String>,
  #[clap(long)]
  pub(crate) hostname: Option<String>,
  #[clap(long = "env")]
  pub(crate) environnements: Option<Vec<String>>,
}
