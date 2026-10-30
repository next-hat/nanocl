use serde::{Deserialize, Serialize};
use std::fs;

use nanocl_error::io::{IoError, IoResult};
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
  pub fn new() -> IoResult<Self> {
    let home_path = std::env::var("HOME").map_err(|_| {
      IoError::other("User configuration", "Could not get $HOME")
    })?;
    let path = format!("{home_path}/.nanocl/conf.yml");
    Self::from_read(fs::read_to_string(path))
  }

  fn from_read(contents: std::io::Result<String>) -> IoResult<Self> {
    match contents {
      Ok(contents) => serde_yaml::from_str(&contents).map_err(|err| {
        IoError::invalid_data("User configuration", &err.to_string())
      }),
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
        Ok(Self::default())
      }
      Err(err) => Err(IoError::with_context("User configuration", err)),
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn user_config_defaults_only_when_missing() {
    let missing =
      UserConfig::from_read(Err(std::io::ErrorKind::NotFound.into())).unwrap();
    assert_eq!(missing.current_context, "default");
    let selected =
      UserConfig::from_read(Ok("CurrentContext: production\n".into())).unwrap();
    assert_eq!(selected.current_context, "production");
    for kind in [
      std::io::ErrorKind::PermissionDenied,
      std::io::ErrorKind::InvalidData,
    ] {
      assert!(UserConfig::from_read(Err(kind.into())).is_err());
    }
    assert!(UserConfig::from_read(Ok("CurrentContext: [".into())).is_err());
  }
}
