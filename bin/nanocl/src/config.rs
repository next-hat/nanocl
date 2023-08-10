use std::fs;
use nanocld_client::NanocldClient;
use serde::{Serialize, Deserialize};

use crate::models::{DisplayFormat, Context};

/// ## CliConfig
///
/// This struct is used to store the user configuration
/// It is stored in the user's home directory in a file located at `.nanocl/conf.yml`
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CliConfig {
  #[serde(default = "default_current_context")]
  pub current_context: String,
  #[serde(default)]
  pub display_format: DisplayFormat,
}

/// ## Default current context
///
/// This function is used to set the default current context
///
fn default_current_context() -> String {
  "default".to_owned()
}

/// ## Default CliConfig
///
/// This is the default configuration used when no configuration file is found
///
impl Default for CliConfig {
  fn default() -> Self {
    Self {
      current_context: default_current_context(),
      display_format: DisplayFormat::Yaml,
    }
  }
}

/// ## CliConfig implementations
///
impl CliConfig {
  /// ## New CliConfig
  ///
  /// This function is used to create a new CliConfig struct
  /// It will read the configuration file located in the user's home directory
  /// If no configuration file is found, it will return the default configuration
  ///
  pub fn new() -> Self {
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
}

/// ## CommandConfig
///
/// A new CommandConfig is created for each command.
/// It is used to pass the configuration to the command functions.
/// And contains the host, the client, the context and the command arguments.
///
pub struct CommandConfig<T> {
  /// Nanocld host to use
  pub host: String,
  /// Nanocld client generated from the host
  pub client: NanocldClient,
  /// Current context
  pub context: Context,
  /// Command arguments
  pub args: T,
  /// User configuration
  pub config: CliConfig,
}
