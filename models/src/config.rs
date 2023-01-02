#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DaemonConfig {
  pub hosts: Vec<String>,
  pub state_dir: String,
  #[cfg_attr(feature = "serde", serde(default = "default_host"))]
  pub docker_host: String,
}

#[derive(Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DaemonConfigFile {
  pub hosts: Option<Vec<String>>,
  pub docker_host: Option<String>,
  pub state_dir: Option<String>,
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
