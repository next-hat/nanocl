use std::fs;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CliConfig {
  pub host: String,
  pub ssl_cert: Option<String>,
  pub ssl_key: Option<String>,
  pub ssl_ca: Option<String>,
}

impl Default for CliConfig {
  fn default() -> Self {
    Self {
      host: String::from("unix://run/nanocl/nanocl.sock"),
      ssl_cert: None,
      ssl_key: None,
      ssl_ca: None,
    }
  }
}

pub fn read() -> CliConfig {
  // Get user config path
  let home_path = match std::env::var("HOME") {
    Ok(home) => home,
    Err(_) => return CliConfig::default(),
  };

  let path = format!("{}/.nanocl.conf", home_path);

  let s = match fs::read_to_string(path) {
    Ok(s) => s,
    Err(_) => return CliConfig::default(),
  };

  match serde_yaml::from_str::<CliConfig>(&s) {
    Ok(config) => config,
    Err(_) => CliConfig::default(),
  }
}
