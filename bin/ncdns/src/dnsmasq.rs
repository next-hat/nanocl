use std::{fs, process::Command, time::Duration};

use ntex::rt;
use ntex::web;

use nanocl_error::io::{FromIo, IoResult};

/// Dnsmasq configuration manager
#[derive(Clone)]
pub struct Dnsmasq {
  pub(crate) config_dir: String,
  pub(crate) config_path: String,
  pub(crate) pid_path: String,
  pub(crate) dns: Vec<String>,
}

impl Dnsmasq {
  /// Create a new Dnsmasq instance
  pub(crate) fn new(config_path: &str) -> Self {
    Self {
      config_dir: config_path.to_owned(),
      config_path: format!("{}/dnsmasq.conf", &config_path.to_owned()),
      pid_path: format!("{}/dnsmasq.pid", &config_path.to_owned()),
      dns: Vec::new(),
    }
  }

  /// Set the dns server to use for resolving domain name if not existing in local
  pub(crate) fn with_dns(&mut self, dns: Vec<String>) -> Self {
    self.dns = dns;
    Self {
      config_dir: self.config_dir.to_owned(),
      config_path: self.config_path.to_owned(),
      pid_path: self.pid_path.to_owned(),
      dns: self.dns.to_owned(),
    }
  }

  /// Write the main dnsmasq config
  #[inline]
  fn write_main_conf(&self, data: &str) -> IoResult<()> {
    fs::write(&self.config_path, data).map_err(|err| {
      err.map_err_context(|| {
        format!("unable to write default config file {}", &self.config_path)
      })
    })?;
    Ok(())
  }

  /// Read the main dnsmasq config
  #[inline]
  fn read_main_conf(&self) -> IoResult<String> {
    let data = fs::read_to_string(&self.config_path).map_err(|err| {
      err.map_err_context(|| {
        format!("unable to read default config file {}", &self.config_path)
      })
    })?;
    Ok(data)
  }

  /// Generate the main dnsmasq config
  /// This config is used to require all other configs from the dnsmasq.d directory
  #[inline]
  fn gen_main_conf(&self) -> IoResult<()> {
    let contents = format!(
      "bind-dynamic
no-resolv
no-poll
no-hosts
proxy-dnssec
except-interface=lo
interface=nanoclbr0
pid-file={}
conf-dir={}/dnsmasq.d,*.conf
",
      &self.pid_path, &self.config_dir
    );
    self.write_main_conf(&contents)?;
    Ok(())
  }

  fn is_running(&self) -> bool {
    let Ok(pid) = fs::read_to_string(&self.pid_path) else {
      return false;
    };
    Command::new("kill")
      .arg("-0")
      .arg(pid.trim())
      .status()
      .is_ok_and(|status| status.success())
  }

  pub(crate) async fn start(&self) -> IoResult<()> {
    let dnsmasq = self.clone();
    web::block(move || {
      if dnsmasq.is_running() {
        return Ok(());
      }
      let _ = fs::remove_file(&dnsmasq.pid_path);
      let output = Command::new("dnsmasq")
        .arg(format!("--conf-file={}", dnsmasq.config_path))
        .arg("--log-facility=-")
        .output()
        .map_err(|err| err.map_err_context(|| "unable to start dnsmasq"))?;
      if output.status.success() {
        return Ok(());
      }
      let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
      message.push_str(String::from_utf8_lossy(&output.stderr).as_ref());
      Err(nanocl_error::io::IoError::other("dnsmasq", &message))
    })
    .await?;
    Ok(())
  }

  pub(crate) fn spawn_monitor(&self) {
    let dnsmasq = self.clone();
    rt::spawn(async move {
      loop {
        if !dnsmasq.is_running() {
          log::warn!("dnsmasq::monitor: dnsmasq is not running, restarting");
          if let Err(err) = dnsmasq.start().await {
            log::warn!("dnsmasq::monitor: {err}");
          }
        }
        ntex::time::sleep(Duration::from_secs(2)).await;
      }
    });
  }

  pub(crate) async fn reload(&self) -> IoResult<()> {
    if !self.is_running() {
      return self.start().await;
    }
    let pid_path = self.pid_path.clone();
    web::block(move || {
      let pid = fs::read_to_string(&pid_path).map_err(|err| {
        err.map_err_context(|| format!("unable to read pid file {pid_path}"))
      })?;
      let output = Command::new("kill")
        .arg("-HUP")
        .arg(pid.trim())
        .output()
        .map_err(|err| err.map_err_context(|| "unable to reload dnsmasq"))?;
      if output.status.success() {
        return Ok(());
      }
      let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
      message.push_str(String::from_utf8_lossy(&output.stderr).as_ref());
      Err(nanocl_error::io::IoError::other("dnsmasq", &message))
    })
    .await?;
    Ok(())
  }

  /// Ensure that dnsmasq as a minimal config
  #[inline]
  pub(crate) fn ensure(&self) -> IoResult<Self> {
    log::info!("dnsmasq::ensure: minimal config {}", &self.config_dir);
    fs::create_dir_all(format!("{}/dnsmasq.d", &self.config_dir)).map_err(
      |err| {
        err.map_err_context(|| {
          format!(
            "unable to create dnsmasq.d directory inside {}",
            &self.config_dir
          )
        })
      },
    )?;
    self.gen_main_conf()?;
    self.set_dns()?;
    log::info!("dnsmasq::ensure: done");
    Ok(self.clone())
  }

  /// Set dns server address to resolve domain name if not existing in local
  #[inline]
  pub(crate) fn set_dns(&self) -> IoResult<()> {
    let data = match self.read_main_conf() {
      Err(_err) => {
        self.gen_main_conf()?;
        self.read_main_conf()?
      }
      Ok(data) => data,
    };
    let lines = data.lines();
    let mut new_data = String::new();
    for dns in &self.dns {
      new_data.push_str(format!("server={dns}\n").as_str());
    }
    for line in lines {
      if line.starts_with("server=") {
        continue;
      }
      new_data.push_str(&format!("{line}\n"));
    }
    self.write_main_conf(&new_data)?;
    Ok(())
  }

  /// Generate domain records file for dnsmasq
  #[inline]
  pub(crate) async fn write_config(
    &self,
    name: &str,
    data: &str,
  ) -> IoResult<()> {
    let file_path = format!("{}/dnsmasq.d/{name}.conf", &self.config_dir);
    log::debug!("dnsmasq::write_config: {file_path}\n{data}");
    tokio::fs::write(file_path, data).await.map_err(|err| {
      err.map_err_context(|| format!("unable to write domains file for {name}"))
    })?;
    Ok(())
  }

  pub(crate) async fn read_config(&self, name: &str) -> IoResult<String> {
    let file_path = format!("{}/dnsmasq.d/{name}.conf", &self.config_dir);
    log::debug!("dnsmasq::read_config: {file_path}");
    let content =
      tokio::fs::read_to_string(file_path).await.map_err(|err| {
        err
          .map_err_context(|| format!("unable to read domains file for {name}"))
      })?;
    Ok(content)
  }

  /// Remove domain records file for dnsmasq
  pub(crate) async fn remove_config(&self, name: &str) -> IoResult<()> {
    let file_path = format!("{}/dnsmasq.d/{name}.conf", &self.config_dir);
    log::debug!("dnsmasq::remove_config: {file_path}");
    tokio::fs::remove_file(file_path).await.map_err(|err| {
      err
        .map_err_context(|| format!("unable to remove domains file for {name}"))
    })?;
    Ok(())
  }
}
