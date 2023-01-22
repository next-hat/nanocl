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
}

impl Default for DaemonConfig {
  fn default() -> Self {
    Self {
      docker_host: default_host(),
      hosts: vec!["/run/nanocl.sock".into()],
      state_dir: "/var/lib/nanocl".into(),
    }
  }
}

fn default_host() -> String {
  "/run/docker.sock".to_owned()
}
