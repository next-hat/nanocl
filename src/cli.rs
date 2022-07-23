use std::io;
use clap_complete::{generate, Generator};
use clap::{App, AppSettings, Parser, Subcommand};

use crate::nanocld::{
  git_repository::GitRepositoryPartial,
  namespace::NamespacePartial,
  cluster::{ClusterPartial, ClusterNetworkPartial, ClusterVarPartial},
  cargo::CargoPartial,
  container_image::ContainerImagePartial,
  nginx_template::NginxTemplateModes,
  container::ListContainerOptions,
};

/// A self-sufficient hybrid-cloud manager
#[derive(Debug, Parser)]
#[clap(
  about,
  version,
  name = "nanocl",
  long_about = "test",
  global_setting = AppSettings::DeriveDisplayOrder,
)]
pub struct Cli {
  /// Nanocld host
  #[clap(long, short = 'H', default_value = "unix://run/nanocl/nanocl.sock")]
  pub host: String,
  /// Commands
  #[clap(subcommand)]
  pub command: Commands,
}

/// Namespace commands
#[derive(Debug, Subcommand)]
pub enum NamespaceCommands {
  /// Create new namespace
  Create(NamespacePartial),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List,
}

/// Git repository delete options
#[derive(Debug, Parser)]
pub struct GitRepositoryDeleteOptions {
  /// Name of repository to delete
  pub name: String,
}

/// Cluster delete options
#[derive(Debug, Parser)]
pub struct ClusterDeleteOptions {
  /// Name of cluster to delete
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

/// Cluster start options
#[derive(Debug, Parser)]
pub struct ClusterStartOptions {
  /// Name of cluster to start
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct ClusterInspectOptions {
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct ClusterNginxTemplateCommandsOption {
  /// Name of cluster
  pub(crate) cl_name: String,
  /// Name of nginx template
  pub(crate) nt_name: String,
}

#[derive(Debug, Subcommand)]
pub enum ClusterNginxTemplateCommands {
  /// Add a new template
  Add(ClusterNginxTemplateCommandsOption),
  /// Remove a existing template
  #[clap(alias("rm"))]
  Remove(ClusterNginxTemplateCommandsOption),
}

/// Control cluster nginx templates
#[derive(Debug, Parser)]
pub struct ClusterNginxTemplateArgs {
  #[clap(subcommand)]
  pub(crate) commands: ClusterNginxTemplateCommands,
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
pub struct ClusterVariableOptions {
  pub(crate) cluster: String,
  #[clap(subcommand)]
  pub(crate) commands: ClusterVariableCommands,
}

/// Cluster sub commands
#[derive(Debug, Subcommand)]
pub enum ClusterCommands {
  /// List existing cluster
  #[clap(alias("ls"))]
  List,
  /// Create new cluster
  Create(ClusterPartial),
  /// Remove cluster by it's name
  #[clap(alias("rm"))]
  Remove(ClusterDeleteOptions),
  /// Start cluster by it's name
  Start(ClusterStartOptions),
  /// Inspect cluster by it's name
  Inspect(ClusterInspectOptions),
  /// Control cluster nginx templates
  NginxTemplate(ClusterNginxTemplateArgs),
  /// Control cluster networks
  Network(ClusterNetworkArgs),
  /// Control cluster variables
  Variable(ClusterVariableOptions),
}

/// Cluster network delete topions
#[derive(Debug, Parser)]
pub struct ClusterNetworkDeleteOptions {
  /// Name of the cluster where network is
  pub cluster_name: String,
  /// Name of the network
  pub name: String,
}

/// Cluster network option
#[derive(Debug, Parser)]
pub struct ClusterNetworkOptions {
  /// Name of the cluster where network is
  #[clap(long)]
  pub cluster_name: String,
}

/// Cluster network commads
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
  /// Inspect a cargo
  Inspect(CargoInspectOption),
}

/// manage cargoes
#[derive(Debug, Parser)]
#[clap(name = "nanocl-cargo")]
pub struct CargoArgs {
  /// namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: CargoCommands,
}

/// alias to self-managed dockerd
#[derive(Debug, Parser)]
pub struct DockerOptions {
  #[clap(multiple = true, raw = true)]
  pub args: Vec<String>,
}

/// manage namespaces
#[derive(Debug, Parser)]
#[clap(name = "nanocl-namespace")]
pub struct NamespaceArgs {
  #[clap(subcommand)]
  pub commands: NamespaceCommands,
}

/// manage git repositories
#[derive(Debug, Parser)]
pub struct GitRepositoryArgs {
  /// namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: GitRepositoryCommands,
}

/// manage clusters
#[derive(Debug, Parser)]
pub struct ClusterArgs {
  /// namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: ClusterCommands,
}

/// manage cluster networks
#[derive(Debug, Parser)]
pub struct ClusterNetworkArgs {
  /// cluster to target
  pub cluster: String,
  #[clap(subcommand)]
  pub commands: ClusterNetworkCommands,
}

/// apply a configuration file
#[derive(Debug, Parser)]
#[clap(name = "nanocl-apply")]
pub struct ApplyArgs {
  #[clap(short)]
  /// .yml conf file to apply
  pub(crate) file_path: String,
}

/// revert a configuration file
#[derive(Debug, Parser)]
#[clap(name = "nanocl-revert")]
pub struct RevertArgs {
  #[clap(short)]
  /// .yml conf file to revert
  pub(crate) file_path: String,
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

/// Manage nginx templates
#[derive(Debug, Parser)]
pub struct NginxTemplateArgs {
  #[clap(subcommand)]
  pub(crate) commands: NginxTemplateCommand,
}

#[derive(Debug, Parser)]
pub struct ContainerImageRemoveOpts {
  /// id or name of image to delete
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct ContainerImageDeployOpts {
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum ContainerImageCommands {
  #[clap(alias("ls"))]
  List,
  Create(ContainerImagePartial),
  #[clap(alias("rm"))]
  Remove(ContainerImageRemoveOpts),
  #[clap(alias("dp"))]
  Deploy(ContainerImageDeployOpts),
}

/// Manage container images
#[derive(Debug, Parser)]
pub struct ContainerImageArgs {
  #[clap(subcommand)]
  pub(crate) commands: ContainerImageCommands,
}

/// Run a cargo in given environement
#[derive(Debug, Parser)]
pub struct RunArgs {
  #[clap(long)]
  pub(crate) namespace: Option<String>,
  #[clap(long)]
  pub(crate) cluster: String,
  #[clap(long)]
  pub(crate) network: String,
  #[clap(long)]
  pub(crate) image: String,
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
  Docker(DockerOptions),
  Namespace(NamespaceArgs),
  Cluster(ClusterArgs),
  Cargo(CargoArgs),
  Apply(ApplyArgs),
  Revert(RevertArgs),
  GitRepository(GitRepositoryArgs),
  NginxTemplate(NginxTemplateArgs),
  ContainerImage(ContainerImageArgs),
  #[clap(name = "lsc")]
  ListContainer(ListContainerOptions),
  Run(RunArgs),
  /// Connect to nginx logging
  NginxLog,
  /// Show the Nanocl version information
  Version,
  // TODO shell ompletion
  // Completion {
  //   /// Shell to generate completion for
  //   #[clap(arg_enum)]
  //   shell: Shell,
  // },
}

// TODO for completion
pub fn _print_completion<G>(gen: G, app: &mut App)
where
  G: Generator,
{
  generate(gen, app, app.get_name().to_string(), &mut io::stdout());
}
