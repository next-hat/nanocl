#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Configuration of the daemon
/// It is used to configure the daemon
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DaemonConfig {
  /// List of hosts to listen on
  pub hosts: Vec<String>,
  /// Path to the state directory
  pub state_dir: String,
  /// Docker host to use
  #[cfg_attr(feature = "serde", serde(default = "default_host"))]
  pub docker_host: String,
  /// Host gateway automatically detected to host default gateway if not set
  pub gateway: String,
  /// Hostname to use for the node automatically detected if not set
  pub hostname: String,
  /// List of nodes to join
  pub nodes: Vec<String>,
  /// Address to advertise to other nodes
  pub advertise_addr: String,
}

/// Configuration File of the daemon
/// It is used to configure the daemon from a file
#[derive(Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DaemonConfigFile {
  /// List of hosts to listen on
  pub hosts: Option<Vec<String>>,
  /// Path to the state directory
  pub state_dir: Option<String>,
  /// Docker host to use
  pub docker_host: Option<String>,
  /// Host gateway automatically detected to host default gateway if not set
  pub gateway: Option<String>,
  /// Hostname to use for the node automatically detected if not set
  pub hostname: Option<String>,
}

impl Default for DaemonConfig {
  fn default() -> Self {
    Self {
      docker_host: default_host(),
      hostname: String::default(),
      hosts: vec!["/run/nanocl.sock".into()],
      state_dir: "/var/lib/nanocl".into(),
      gateway: String::default(),
      nodes: Vec::default(),
      advertise_addr: String::default(),
    }
  }
}

fn default_host() -> String {
  "/run/docker.sock".to_owned()
}
