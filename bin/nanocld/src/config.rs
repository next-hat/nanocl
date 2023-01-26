use nanocl_stubs::config::{DaemonConfig, DaemonConfigFile};

use crate::cli::Cli;
use crate::error::DaemonError;

fn merge_config(args: &Cli, config: &DaemonConfigFile) -> DaemonConfig {
  let hosts = if let Some(ref hosts) = args.hosts {
    hosts.to_owned()
  } else if let Some(ref hosts) = config.hosts {
    hosts.to_owned()
  } else {
    vec![String::from("unix:///run/nanocl/nanocl.sock")]
  };

  let state_dir = if let Some(ref state_dir) = args.state_dir {
    state_dir.to_owned()
  } else if let Some(ref state_dir) = config.state_dir {
    state_dir.to_owned()
  } else {
    String::from("/var/lib/nanocl")
  };

  let docker_host = if let Some(ref docker_host) = args.docker_host {
    docker_host.to_owned()
  } else if let Some(ref docker_host) = config.docker_host {
    docker_host.to_owned()
  } else {
    String::from("/run/docker.sock")
  };

  DaemonConfig {
    hosts,
    state_dir,
    docker_host,
  }
}

fn read_config_file(
  config_dir: &String,
) -> Result<DaemonConfigFile, DaemonError> {
  let config_path = std::path::Path::new(&config_dir).join("nanocl.conf");

  if !config_path.exists() {
    return Ok(DaemonConfigFile::default());
  }

  let content = std::fs::read_to_string(&config_path)?;
  let config = serde_yaml::from_str::<DaemonConfigFile>(&content)?;

  Ok(config)
}

/// ## Init Daemon config
///
/// Init Daemon config
/// It will read /etc/nanocl/nanocl.conf
/// and parse Cli arguments we merge them together with a priority to Cli arguments
///
/// ## Arguments
///
/// - [args](Cli) - Cli arguments
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](DaemonConfig) - The created cargo config
///   - [Err](DaemonError) - Error during the operation
///
/// ## Example
///
/// ```rust,norun
/// use crate::cli::Cli;
/// use crate::state::config;
///
/// let args = Cli {
///   hosts: Some(vec![String::from("unix:///run/nanocl/nanocl.sock")]),
///   state_dir: Some(String::from("/var/lib/nanocl")),
///   docker_host: Some(String::from("/run/docker.sock")),
/// };
///
/// let result = config::init(args);
/// ```
///
pub fn init(args: &Cli) -> Result<DaemonConfig, DaemonError> {
  let file_config = read_config_file(&args.config_dir)?;

  // Merge cli args and config file with priority to args
  Ok(merge_config(args, &file_config))
}

/// Config unit test
#[cfg(test)]
mod tests {
  use std::os::unix::prelude::PermissionsExt;

  use super::*;

  /// Test merge config
  #[test]
  fn test_merge_config() {
    let args = Cli {
      hosts: Some(vec![String::from("unix:///run/nanocl/nanocl.sock")]),
      state_dir: Some(String::from("/var/lib/nanocl")),
      docker_host: Some(String::from("/run/docker.sock")),
      config_dir: String::from("/etc/nanocl"),
      init: false,
    };

    let config = DaemonConfigFile {
      hosts: Some(vec![String::from("unix:///run/nanocl/nanocl.sock")]),
      state_dir: Some(String::from("/var/lib/nanocl")),
      docker_host: Some(String::from("/run/docker.sock")),
    };

    let merged = merge_config(&args, &config);

    assert_eq!(merged.hosts, args.hosts.unwrap());
    assert_eq!(merged.state_dir, args.state_dir.unwrap());
    assert_eq!(merged.docker_host, args.docker_host.unwrap());
  }

  /// Test read config file
  /// It should return a default config if the file does not exist
  /// It should return a config if the file exist
  /// It should return an error if the file is not a valid yaml
  /// It should return an error if the file is not readable
  /// It should return an error if the file is not a file
  #[test]
  fn test_read_config_file() {
    let config_dir = String::from("/tmp");
    let config_path = std::path::Path::new(&config_dir).join("nanocl.conf");

    // Ensure the test file is removed
    if config_path.exists() {
      std::fs::remove_file(&config_path).unwrap();
    }

    // It should return a default config if the file does not exist
    let config = read_config_file(&config_dir);
    assert!(config.is_ok());
    assert_eq!(config.unwrap(), DaemonConfigFile::default());

    // It should return a config if the file exist
    let content = r#"state_dir: /var/lib/nanocl"#;
    std::fs::write(&config_path, content).unwrap();
    let config = read_config_file(&config_dir);
    assert!(config.is_ok());
    assert_eq!(
      config.unwrap(),
      DaemonConfigFile {
        state_dir: Some(String::from("/var/lib/nanocl")),
        ..Default::default()
      }
    );
    // It should return an error if the file is not a valid yaml
    let content = r#"state_dir; /var/lib/nanocl\n"#;
    std::fs::write(&config_path, content).unwrap();
    let config = read_config_file(&config_dir);
    assert!(config.is_err());

    // It should return an error if the file is not readable
    std::fs::set_permissions(
      &config_path,
      std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let config = read_config_file(&config_dir);
    assert!(config.is_err());

    // It should return an error if the file is not a file
    std::fs::remove_file(&config_path).unwrap();
    std::fs::create_dir(&config_path).unwrap();
    let config = read_config_file(&config_dir);
    assert!(config.is_err());
    std::fs::remove_dir_all(&config_path).unwrap();
  }

  /// Test init config
  #[test]
  fn test_init_config() {
    let args = Cli {
      hosts: Some(vec![String::from("unix:///run/nanocl/nanocl.sock")]),
      state_dir: Some(String::from("/var/lib/nanocl")),
      docker_host: Some(String::from("/run/docker.sock")),
      config_dir: String::from("/etc/nanocl"),
      init: false,
    };

    let config = init(&args).unwrap();
    assert_eq!(config.hosts, args.hosts.unwrap());
    assert_eq!(config.state_dir, args.state_dir.unwrap());
    assert_eq!(config.docker_host, args.docker_host.unwrap());
  }
}
