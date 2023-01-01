use std::path::{Path, PathBuf};

use regex::Regex;
use ntex::http::StatusCode;
use tokio::{fs, io::AsyncWriteExt};
use bollard::Docker;

use thiserror::Error;
use regex::Error as RegexError;
use std::io::Error as IoError;
use bollard::errors::Error as DockerError;

use nanocl_models::cargo::CargoPartial;
use nanocl_models::cargo_config::CargoConfigPartial;

use crate::{utils, repositories};
use crate::errors::{HttpResponseError, IntoHttpResponseError, DaemonError};
use crate::models::ArgState;

use crate::utils::errors::docker_error_ref;

#[derive(Debug, Error)]
pub enum DnsError {
  #[error("dnsmasq io error")]
  Io(#[from] IoError),
  #[error("dnsmasq regex error")]
  Regex(#[from] RegexError),
  #[error("dnsmasq docker_api error")]
  Docker(#[from] DockerError),
}

impl IntoHttpResponseError for DnsError {
  fn to_http_error(&self) -> HttpResponseError {
    match self {
      DnsError::Io(err) => HttpResponseError {
        msg: format!("dnsmasq io error {:#?}", err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      },
      DnsError::Regex(err) => HttpResponseError {
        msg: format!("dnsmasq regex error {:#?}", err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      },
      DnsError::Docker(err) => docker_error_ref(err),
    }
  }
}

/// Write content into given path
///
/// ## Arguments
/// - [path](PathBuf) The file path to write in
/// - [content](str) The content to write as a string reference
async fn write_dns_entry_conf(
  path: &PathBuf,
  content: &str,
) -> std::io::Result<()> {
  let mut f = fs::File::create(path).await?;
  f.write_all(content.as_bytes()).await?;
  f.sync_data().await?;
  Ok(())
}

/// Write default dns config if not exists.
async fn write_dns_default_conf(path: &PathBuf) -> std::io::Result<()> {
  if path.exists() {
    return Ok(());
  }
  let content = "bind-interfaces\n \
interface=nanoclinternal0\n \
server=8.8.8.8\n \
server=8.8.4.4\n \
conf-dir=/etc/dnsmasq.d/,*.conf\n"
    .to_owned();
  let mut f = fs::File::create(path).await?;
  f.write_all(content.as_bytes()).await?;
  f.sync_data().await?;
  Ok(())
}

/// Add or Update a dns entry
///
/// ## Arguments
/// - [domain_name](str) The domain name to add
/// - [ip_address](str) The ip address the domain target
/// - [state_dir](str) Daemon state dir to know where to store the information
pub async fn add_dns_entry(
  domain_name: &str,
  ip_address: &str,
  state_dir: &str,
) -> Result<(), DnsError> {
  let file_path = Path::new(state_dir).join("dnsmasq/dnsmasq.d/dns_entry.conf");
  if !file_path.exists() {
    fs::create_dir_all(file_path.parent().ok_or_else(|| {
      std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Parent directory not found".to_string(),
      )
    })?)
    .await?;
    fs::File::create(&file_path).await?;
  }
  let content = fs::read_to_string(&file_path).await?;
  let reg_expr = r"address=/.".to_owned() + domain_name + "/.*\\n";
  let reg = Regex::new(&reg_expr)?;
  let new_dns_entry =
    "address=/.".to_owned() + domain_name + "/" + ip_address + "\n";
  if reg.is_match(&content) {
    // If entry exist we just update it by replacing the ip address
    let res = reg.replace_all(&content, &new_dns_entry);
    let new_content = res.to_string();
    write_dns_entry_conf(&file_path, &new_content).await?;
  } else {
    // else we just add it at end of file.
    let mut file = fs::OpenOptions::new()
      .write(true)
      .append(true)
      .open(file_path)
      .await?;
    file.write_all(new_dns_entry.as_bytes()).await?;
  }

  Ok(())
}

/// Restart dns controller
///
/// ## Arguments
/// - [docker_api](Docker) Docker api reference
pub async fn restart(docker_api: &Docker) -> Result<(), DnsError> {
  docker_api
    .restart_container("system-nano-dns", None)
    .await?;
  Ok(())
}

/// Register dns controller as a cargo
/// That way we can manage it using nanocl commands
///
/// ## Arguments
/// - [arg](ArgState) Reference to argument state
pub async fn register(arg: &ArgState) -> Result<(), DaemonError> {
  let key = utils::key::gen_key(&arg.sys_namespace, "dns");

  if repositories::cargo::find_by_key(key, &arg.pool)
    .await
    .is_ok()
  {
    return Ok(());
  }

  let dir_path = Path::new(&arg.config.state_dir).join("dnsmasq");

  if !dir_path.exists() {
    fs::create_dir_all(&dir_path).await?;
  }

  let config_file_path = Path::new(&dir_path).join("dnsmasq.conf");
  write_dns_default_conf(&config_file_path).await?;
  let dir_path = Path::new(&dir_path).join("dnsmasq.d/");
  let binds = Some(vec![
    format!("{}:/etc/dnsmasq.conf", config_file_path.display()),
    format!("{}:/etc/dnsmasq.d/", dir_path.display()),
  ]);

  let container = bollard::container::Config {
    image: Some(String::from("nanocl-dns:0.0.2")),
    hostname: Some(String::from("dns")),
    domainname: Some(String::from("dns")),
    host_config: Some(bollard::models::HostConfig {
      binds,
      network_mode: Some(String::from("host")),
      restart_policy: Some(bollard::models::RestartPolicy {
        name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
        ..Default::default()
      }),
      cap_add: Some(vec![String::from("NET_ADMIN")]),
      ..Default::default()
    }),
    ..Default::default()
  };

  let config = CargoConfigPartial {
    name: String::from("dns"),
    container,
    ..Default::default()
  };

  let dns_cargo = CargoPartial {
    name: String::from("dns"),
    config,
  };

  repositories::cargo::create(
    arg.sys_namespace.to_owned(),
    dns_cargo,
    &arg.pool,
  )
  .await?;

  Ok(())
}

#[cfg(test)]
mod tests {

  use std::env;

  use super::*;

  use crate::utils::tests::*;

  struct TestDomain {
    name: String,
    ip_address: String,
  }

  /// Test write default dns config file
  #[ntex::test]
  async fn test_write_dns_default_conf() {
    let config_file_path = Path::new("/tmp").join("dnsmasq.conf");
    if config_file_path.exists() {
      fs::remove_file(&config_file_path).await.unwrap();
    }
    write_dns_default_conf(&config_file_path).await.unwrap();
    assert!(config_file_path.exists());
    let content = fs::read_to_string(&config_file_path).await.unwrap();
    assert_eq!(
      content,
      "bind-interfaces\n \
      interface=nanoclinternal0\n \
      server=8.8.8.8\n \
      server=8.8.4.4\n \
      conf-dir=/etc/dnsmasq.d/,*.conf\n"
    );
    fs::remove_file(config_file_path).await.unwrap();
  }

  #[ntex::test]
  async fn manipulate_dns_entry() -> TestRet {
    // Create temporary directory for the tests
    let tmp_state_dir =
      env::temp_dir().join("nanocld-unit").display().to_string();
    let dnsmasq_conf_dir = Path::new(&tmp_state_dir).join("dnsmasq/dnsmasq.d");
    fs::create_dir_all(&dnsmasq_conf_dir).await?;

    // Create a dummy dns entry file
    let dns_entry_path = Path::new(&dnsmasq_conf_dir).join("dns_entry.conf");
    write_dns_entry_conf(&dns_entry_path, "").await?;

    // Test to add domain test.com pointing to 141.0.0.1
    let test_1 = TestDomain {
      name: String::from("test.com"),
      ip_address: String::from("141.0.0.1"),
    };
    add_dns_entry(&test_1.name, &test_1.ip_address, &tmp_state_dir).await?;
    let content = fs::read_to_string(&dns_entry_path).await?;
    let expected_content =
      format!("address=/.{}/{}\n", &test_1.name, &test_1.ip_address);
    assert_eq!(content, expected_content);

    // Test to add another domain test2.com pointing to 122.0.0.1
    let test_2 = TestDomain {
      name: String::from("test2.com"),
      ip_address: String::from("122.0.0.1"),
    };
    add_dns_entry(&test_2.name, &test_2.ip_address, &tmp_state_dir).await?;
    let content = fs::read_to_string(&dns_entry_path).await?;
    let expected_content = format!(
      "address=/.{}/{}\naddress=/.{}/{}\n",
      &test_1.name, &test_1.ip_address, &test_2.name, &test_2.ip_address
    );
    assert_eq!(content, expected_content);

    // Test to update domain 2 with a new ip address 121.0.0.1
    let test_3 = TestDomain {
      ip_address: String::from("121.0.0.1"),
      ..test_2
    };
    add_dns_entry(&test_3.name, &test_3.ip_address, &tmp_state_dir).await?;
    let content = fs::read_to_string(&dns_entry_path).await?;
    let expected_content = format!(
      "address=/.{}/{}\naddress=/.{}/{}\n",
      &test_1.name, &test_1.ip_address, &test_3.name, &test_3.ip_address
    );
    assert_eq!(content, expected_content);

    // Remove the dummy state directory
    fs::remove_dir_all(&tmp_state_dir).await?;
    Ok(())
  }
}
