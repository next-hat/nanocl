use clap::Parser;

#[derive(Debug, Clone, Parser)]
pub struct SetupOpts {
  #[clap(long)]
  pub(crate) docker_host: Option<String>,
  #[clap(long)]
  pub(crate) state_dir: Option<String>,
  #[clap(long)]
  pub(crate) conf_dir: Option<String>,
  #[clap(long)]
  pub(crate) gateway: Option<String>,
  #[clap(long)]
  pub(crate) deamon_hosts: Option<Vec<String>>,
  #[clap(long)]
  pub(crate) group: Option<String>,
  #[clap(long, default_value = "0.3.0")]
  pub(crate) version: String,
  #[clap(long)]
  pub(crate) hostname: Option<String>,
}

/// This is the struct that will be passed to nanocl daemon
#[allow(unused)]
pub struct NanocldArgs {
  pub(crate) docker_host: String,
  pub(crate) state_dir: String,
  pub(crate) conf_dir: String,
  pub(crate) gateway: String,
  pub(crate) hosts: Vec<String>,
  pub(crate) hostname: String,
  pub(crate) gid: u32,
  pub(crate) version: String,
}
