use nanocl_stubs::config::{DaemonConfig, DaemonConfigFile};

use crate::utils;
use crate::cli::Cli;
use crate::error::CliError;

fn gen_daemon_conf(
  args: &Cli,
  config: &DaemonConfigFile,
) -> Result<DaemonConfig, CliError> {
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

  let gateway = if let Some(ref gateway) = args.gateway {
    gateway.to_owned()
  } else if let Some(ref gateway) = config.gateway {
    gateway.to_owned()
  } else {
    utils::network::get_default_ip()
      .map_err(|err| {
        CliError::new(1, format!("Error while getting default ip: {err}"))
      })?
      .to_string()
  };

  let hostname = if let Some(ref hostname) = args.hostname {
    hostname.to_owned()
  } else if let Some(ref hostname) = config.hostname {
    hostname.to_owned()
  } else {
    utils::network::get_hostname().map_err(|err| {
      CliError::new(1, format!("Error while getting hostname: {err}"))
    })?
  };

  Ok(DaemonConfig {
    hosts,
    state_dir,
    docker_host,
    gateway,
    hostname,
    nodes: args.nodes.clone(),
  })
}

fn read_config_file(config_dir: &String) -> Result<DaemonConfigFile, CliError> {
  let config_path = std::path::Path::new(&config_dir).join("nanocl.conf");

  if !config_path.exists() {
    return Ok(DaemonConfigFile::default());
  }

  let content = std::fs::read_to_string(&config_path).map_err(|err| {
    CliError::new(
      1,
      format!(
        "Error while reading config file at {}: {err}",
        config_path.display()
      ),
    )
  })?;
  let config =
    serde_yaml::from_str::<DaemonConfigFile>(&content).map_err(|err| {
      CliError::new(
        1,
        format!(
          "Error while parsing config file at {}: {err}",
          config_path.display()
        ),
      )
    })?;

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
pub fn init(args: &Cli) -> Result<DaemonConfig, CliError> {
  let file_config = read_config_file(&args.conf_dir)?;
  // Merge cli args and config file with priority to args
  gen_daemon_conf(args, &file_config)
}

/// Config unit test
#[cfg(test)]
mod tests {
  use std::os::unix::prelude::PermissionsExt;

  use super::*;

  /// Test merge config
  #[test]
  fn merge_config() {
    let args = Cli {
      hosts: Some(vec![String::from("unix:///run/nanocl/nanocl.sock")]),
      state_dir: Some(String::from("/var/lib/nanocl")),
      docker_host: Some(String::from("/run/docker.sock")),
      conf_dir: String::from("/etc/nanocl"),
      init: false,
      gateway: None,
      hostname: None,
      nodes: Vec::default(),
    };

    let config = DaemonConfigFile {
      hosts: Some(vec![String::from("unix:///run/nanocl/nanocl.sock")]),
      state_dir: Some(String::from("/var/lib/nanocl")),
      docker_host: Some(String::from("/run/docker.sock")),
      gateway: None,
      hostname: None,
    };

    let merged = gen_daemon_conf(&args, &config).unwrap();

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
  fn read_from_file() {
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
  fn init_config() {
    let args = Cli {
      hosts: Some(vec![String::from("unix:///run/nanocl/nanocl.sock")]),
      state_dir: Some(String::from("/var/lib/nanocl")),
      docker_host: Some(String::from("/run/docker.sock")),
      conf_dir: String::from("/etc/nanocl"),
      init: false,
      gateway: None,
      hostname: None,
      nodes: Vec::default(),
    };

    let config = init(&args).unwrap();
    assert_eq!(config.hosts, args.hosts.unwrap());
    assert_eq!(config.state_dir, args.state_dir.unwrap());
    assert_eq!(config.docker_host, args.docker_host.unwrap());
  }
}
