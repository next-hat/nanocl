use tabled::Tabled;
use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

use super::cargo::CargoItem;
use super::utils::tabled::*;

/// Manage clusters
#[derive(Debug, Parser)]
#[clap(
  name = "nanocl-cluster",
  long_about = "Create, Update, Inspect or Delete cluster"
)]
pub struct ClusterArgs {
  /// Namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  /// Available subcommands
  #[clap(subcommand)]
  pub commands: ClusterCommands,
}

/// Cluster sub commands
#[derive(Debug, Subcommand)]
pub enum ClusterCommands {
  /// List existing cluster
  #[clap(alias("ls"))]
  List,
  /// Create a new cluster
  Create(ClusterPartial),
  /// Remove one cluster
  #[clap(alias("rm"))]
  Remove(ClusterDeleteOptions),
  /// Start one cluster
  Start(ClusterStartOptions),
  /// Display detailed information on one cluster
  Inspect(ClusterInspectOptions),
  /// Manage cluster networks
  Network(ClusterNetworkArgs),
  /// Manage cluster variables
  Variable(ClusterVariableArgs),
  /// Create cargo instances inside given cluster and network
  Join(ClusterJoinOptions),
}

#[derive(Debug, Parser, Serialize, Deserialize)]
#[clap(name = "nanocl-cluster-create", long_about = "Create a new cluster")]
pub struct ClusterPartial {
  /// Name of the new cluster
  pub name: String,
  /// List of proxy templates to use when deploying a cargo
  #[clap(long)]
  pub proxy_templates: Option<Vec<String>>,
}

#[derive(Debug, Parser)]
pub struct ClusterVariableRemoveOptions {
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum ClusterVariableCommands {
  /// Create a new variable for the cluster
  Create(ClusterVarPartial),
  /// Delete existing variable from the cluster
  #[clap(alias = "rm")]
  Remove(ClusterVariableRemoveOptions),
}

#[derive(Debug, Parser)]
pub struct ClusterVariableArgs {
  pub(crate) cluster: String,
  #[clap(subcommand)]
  pub(crate) commands: ClusterVariableCommands,
}

#[derive(Debug, Parser)]
pub struct ClusterJoinOptions {
  /// Name of the cluster to join
  pub(crate) cluster_name: String,
  /// Name of the network inside the cluster to join
  pub(crate) network_name: String,
  /// Name of the cargo
  pub(crate) cargo_name: String,
}

/// Manage cluster networks
#[derive(Debug, Parser)]
pub struct ClusterNetworkArgs {
  /// cluster to target
  pub cluster: String,
  #[clap(subcommand)]
  pub commands: ClusterNetworkCommands,
}

/// Remove cluster by it's name
#[derive(Debug, Parser)]
#[clap(
  name = "nanocl-cluster-delete",
  long_about = "Remove cluster by it's name with all related relations, note \
  this will also delete your containers."
)]
pub struct ClusterDeleteOptions {
  /// Name of cluster to delete
  pub name: String,
}

/// Start cluster by it's name
#[derive(Debug, Parser)]
#[clap(
  name = "nanocl-cluster-start",
  long_about = "Start a cluster by it's name note: \
  this will create and start all non running joined cargo and reapply \
  proxy and dns settings"
)]
pub struct ClusterStartOptions {
  /// Name of cluster to start
  pub(crate) name: String,
}

/// Inspect cluster by it's name
#[derive(Debug, Parser)]
#[clap(
  name = "nanocl-cluster-inspect",
  long_about = "Get cluster information from it's name in current namespace"
)]
pub struct ClusterInspectOptions {
  pub(crate) name: String,
}

/// Cluster network delete options
#[derive(Debug, Parser)]
pub struct ClusterNetworkDeleteOptions {
  /// Name of the cluster where network is
  pub cluster_name: String,
  /// Name of the network
  pub name: String,
}

/// Cluster network options
#[derive(Debug, Parser)]
pub struct ClusterNetworkOptions {
  /// Name of the cluster where network is
  #[clap(long)]
  pub cluster_name: String,
}

/// Cluster network commands
#[derive(Debug, Subcommand)]
pub enum ClusterNetworkCommands {
  /// List existing cluster network
  #[clap(alias("ls"))]
  List,
  /// Create new cluster network
  Create(ClusterNetworkPartial),
  /// Remove cluster network
  #[clap(alias("rm"))]
  Remove(ClusterNetworkDeleteOptions),
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct ClusterItem {
  pub(crate) key: String,
  pub(crate) namespace: String,
  pub(crate) name: String,
  #[tabled(display_with = "display_vec_string")]
  pub(crate) proxy_templates: Vec<String>,
  // #[tabled(display_with = "display_option")]
  // pub(crate) networks: Option<Vec<ClusterNetworkItem>>,
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct CargoInstanceItem {
  #[tabled(skip)]
  pub(crate) key: String,
  #[tabled(skip)]
  pub(crate) cargo_key: String,
  #[tabled(skip)]
  pub(crate) cluster_key: String,
  pub(crate) network_key: String,
}

/// Cluster item with his relations
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct ClusterItemWithRelation {
  pub(crate) key: String,
  pub(crate) name: String,
  pub(crate) namespace: String,
  #[tabled(display_with = "display_vec_string")]
  pub(crate) proxy_templates: Vec<String>,
  #[tabled(skip)]
  pub(crate) variables: Vec<ClusterVarItem>,
  #[tabled(skip)]
  pub(crate) networks: Option<Vec<ClusterNetworkItem>>,
  #[tabled(skip)]
  pub(crate) cargoes: Option<Vec<CargoItem>>,
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct ClusterNetworkItem {
  pub(crate) key: String,
  pub(crate) name: String,
  pub(crate) cluster_key: String,
  pub(crate) default_gateway: String,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct ClusterVarPartial {
  pub(crate) name: String,
  pub(crate) value: String,
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct ClusterVarItem {
  #[tabled(skip)]
  pub(crate) key: String,
  #[tabled(skip)]
  pub(crate) cluster_key: String,
  pub(crate) name: String,
  pub(crate) value: String,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct ClusterJoinPartial {
  pub(crate) network: String,
  pub(crate) cargo: String,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct ClusterNetworkPartial {
  pub(crate) name: String,
}
