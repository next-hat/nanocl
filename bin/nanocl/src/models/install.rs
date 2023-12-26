use clap::Parser;

/// `nanocl install` available options
#[derive(Clone, Parser)]
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
  /// Force repull of the nanocl components
  #[clap(short = 'p', long)]
  pub(crate) force_pull: bool,
}

/// Arguments for the nanocl daemon used by the install template
#[derive(Clone)]
pub struct NanocldArg {
  /// Docker host to use
  pub(crate) docker_host: String,
  /// State directory to use
  pub(crate) state_dir: String,
  /// Configuration directory to use
  pub(crate) conf_dir: String,
  /// Public ip address of the current node
  pub(crate) gateway: String,
  /// Hosts to connect to
  pub(crate) hosts: Vec<String>,
  /// Hostname of the current node
  pub(crate) hostname: String,
  /// Group id to use
  pub(crate) gid: u32,
  /// Advertise address to broadcast to other nodes
  pub(crate) advertise_addr: String,
  /// Home directory of the current user
  pub(crate) home_dir: String,
  /// Specify if the docker host is docker desktop
  /// detected if docker context is desktop-linux
  pub(crate) is_docker_desktop: bool,
  /// Build channel used
  pub(crate) channel: String,
  /// Specify if the docker host is unix socket
  pub(crate) docker_uds_path: Option<String>,
}

/// Convert Nanocld to liquid::Object
impl From<NanocldArg> for liquid::Object {
  fn from(arg: NanocldArg) -> Self {
    liquid::object!({
      "docker_host": arg.docker_host,
      "state_dir": arg.state_dir,
      "conf_dir": arg.conf_dir,
      "gateway": arg.gateway,
      "hosts": arg.hosts,
      "hostname": arg.hostname,
      "gid": arg.gid,
      "advertise_addr": arg.advertise_addr,
      "is_docker_desktop": arg.is_docker_desktop,
      "home_dir": arg.home_dir,
      "channel": arg.channel,
      "docker_uds_path": arg.docker_uds_path,
    })
  }
}
