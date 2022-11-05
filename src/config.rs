use serde::{Serialize, Deserialize};

use crate::cli::errors::CliError;

#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonConfig {
  #[serde(default = "default_host")]
  pub(crate) docker_host: String,
}

impl Default for DaemonConfig {
  fn default() -> Self {
    Self {
      docker_host: default_host(),
    }
  }
}

fn default_host() -> String {
  "/run/nanocl/docker.sock".to_owned()
}

pub fn read_daemon_config_file(
  config_dir: &String,
) -> Result<DaemonConfig, CliError> {
  let config_path = std::path::Path::new(&config_dir).join("nanocl.conf");
  if !config_path.exists() {
    return Ok(DaemonConfig::default());
  }
  let content = std::fs::read_to_string(&config_path)?;
  let config = serde_yaml::from_str::<DaemonConfig>(&content)?;
  Ok(config)
}
