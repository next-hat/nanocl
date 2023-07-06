use clap::Parser;

#[derive(Debug, Clone, Parser)]
pub struct InstallOpts {
  /// The docker host to install nanocl default is unix:///var/run/docker.sock
  #[clap(long)]
  pub(crate) docker_host: Option<String>,
  /// Specify if the docker host is docker desktop detected if docker context is desktop-linux
  #[clap(long = "docker-desktop")]
  pub(crate) is_docker_desktop: bool,
  /// The state directory to store the state of the nanocl daemon default is /var/lib/nanocl
  #[clap(long)]
  pub(crate) state_dir: Option<String>,
  /// The configuration directory to store the configuration of the nanocl daemon default is /etc/nanocl
  #[clap(long)]
  pub(crate) conf_dir: Option<String>,
  /// The gateway address to use for the nanocl daemon default is detected
  #[clap(long)]
  pub(crate) gateway: Option<String>,
  /// The hosts to use for the nanocl daemon default is detected
  #[clap(long)]
  pub(crate) advertise_addr: Option<String>,
  /// The hosts to use for the nanocl daemon default is detected
  #[clap(long)]
  pub(crate) deamon_hosts: Option<Vec<String>>,
  /// The group to use for the nanocl daemon default is nanocl
  #[clap(long)]
  pub(crate) group: Option<String>,
  /// The hostname to use for the nanocl daemon default is detected
  #[clap(long)]
  pub(crate) hostname: Option<String>,
  /// Installation template to use for nanocl by default it's detected
  #[clap(short, long)]
  pub(crate) template: Option<String>,
}

/// This is the struct that will be passed to nanocl daemon
#[derive(Debug, Clone)]
pub struct NanocldArgs {
  pub(crate) docker_host: String,
  pub(crate) state_dir: String,
  pub(crate) conf_dir: String,
  pub(crate) gateway: String,
  pub(crate) hosts: Vec<String>,
  pub(crate) hostname: String,
  pub(crate) gid: u32,
  pub(crate) advertise_addr: String,
  pub(crate) home_dir: String,
  pub(crate) is_docker_desktop: bool,
}

impl From<NanocldArgs> for liquid::Object {
  fn from(args: NanocldArgs) -> Self {
    liquid::object!({
      "docker_host": args.docker_host,
      "state_dir": args.state_dir,
      "conf_dir": args.conf_dir,
      "gateway": args.gateway,
      "hosts": args.hosts,
      "hostname": args.hostname,
      "gid": args.gid,
      "advertise_addr": args.advertise_addr,
      "is_docker_desktop": args.is_docker_desktop,
      "home_dir": args.home_dir,
    })
  }
}
