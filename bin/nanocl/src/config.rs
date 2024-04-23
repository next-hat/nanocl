use std::fs;
use serde::{Serialize, Deserialize};

use nanocld_client::NanocldClient;

use crate::models::{Context, DisplayFormat};

/// This struct is used to store the user configuration
/// It is stored in the user's home directory in a file located at `.nanocl/conf.yml`
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserConfig {
  #[serde(default = "default_current_context")]
  pub current_context: String,
  #[serde(default)]
  pub display_format: DisplayFormat,
}

/// This function is used to set the default current context
fn default_current_context() -> String {
  "default".to_owned()
}

/// This is the default configuration used when no configuration file is found
impl Default for UserConfig {
  fn default() -> Self {
    Self {
      current_context: default_current_context(),
      display_format: DisplayFormat::Yaml,
    }
  }
}

impl UserConfig {
  /// This function is used to create a new CliConfig struct
  /// It will read the configuration file located in the user's home directory
  /// If no configuration file is found, it will return the default configuration
  pub fn new() -> Self {
    // Get user config path
    let home_path = match std::env::var("HOME") {
      Ok(home) => home,
      Err(_) => return UserConfig::default(),
    };
    let path = format!("{}/.nanocl/conf.yml", home_path);
    let s = match fs::read_to_string(path) {
      Ok(s) => s,
      Err(_) => return UserConfig::default(),
    };
    match serde_yaml::from_str::<UserConfig>(&s) {
      Ok(config) => config,
      Err(_) => UserConfig::default(),
    }
  }
}

/// A new `CliConfig` is created for each command.
/// It is used to pass the configuration to the command functions.
/// And contains the host, the client, the context and the command arguments.
#[derive(Clone)]
pub struct CliConfig {
  /// Nanocld host to use
  pub host: String,
  /// Nanocld client generated from the host
  pub client: NanocldClient,
  /// Current context
  pub context: Context,
  /// User configuration
  pub user_config: UserConfig,
}
