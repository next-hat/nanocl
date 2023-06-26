use std::fs;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CliConfig {
  pub current_context: String,
}

impl Default for CliConfig {
  fn default() -> Self {
    Self {
      current_context: "default".to_string(),
    }
  }
}

pub fn read() -> CliConfig {
  // Get user config path
  let home_path = match std::env::var("HOME") {
    Ok(home) => home,
    Err(_) => return CliConfig::default(),
  };

  let path = format!("{}/.nanocl/conf.yml", home_path);

  let s = match fs::read_to_string(path) {
    Ok(s) => s,
    Err(_) => return CliConfig::default(),
  };

  match serde_yaml::from_str::<CliConfig>(&s) {
    Ok(config) => config,
    Err(_) => CliConfig::default(),
  }
}
