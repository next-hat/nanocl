use nanocl_models::config::DaemonConfig;

use crate::error::CliError;

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
