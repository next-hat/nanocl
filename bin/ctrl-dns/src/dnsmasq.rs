use std::fs;

use crate::error::ErrorHint;

/// Dnsmasq configuration manager
#[derive(Clone)]
pub(crate) struct Dnsmasq {
  pub(crate) config_dir: String,
  pub(crate) config_path: String,
  pub(crate) dns: Vec<String>,
}

impl Dnsmasq {
  /// Create a new Dnsmasq instance
  pub(crate) fn new(config_path: &str) -> Self {
    Self {
      config_dir: config_path.to_owned(),
      config_path: format!("{}/dnsmasq.conf", &config_path.to_owned()),
      dns: Vec::new(),
    }
  }

  /// Set the dns server to use for resolving domain name if not existing in local
  pub(crate) fn with_dns(&mut self, dns: Vec<String>) -> Self {
    self.dns = dns;
    Self {
      config_dir: self.config_dir.to_owned(),
      config_path: self.config_path.to_owned(),
      dns: self.dns.to_owned(),
    }
  }

  /// Write the main dnsmasq config
  #[inline]
  fn write_main_conf(&self, data: &str) -> Result<(), ErrorHint> {
    fs::write(&self.config_path, data).map_err(|err| {
      ErrorHint::Error(format!(
        "unable to create default config file {} got error: {err}",
        &self.config_path
      ))
    })?;
    Ok(())
  }

  /// Read the main dnsmasq config
  #[inline]
  fn read_main_conf(&self) -> Result<String, ErrorHint> {
    let data = fs::read_to_string(&self.config_path).map_err(|err| {
      ErrorHint::Warning(format!(
        "unable to read default config file {} got error: {err}",
        &self.config_path
      ))
    })?;
    Ok(data)
  }

  /// Generate the main dnsmasq config
  /// This config is used to require all other configs from the dnsmasq.d directory
  #[inline]
  fn gen_main_conf(&self) -> Result<(), ErrorHint> {
    let contents = format!("conf-dir={}/dnsmasq.d,*.conf\n", &self.config_dir);
    self.write_main_conf(&contents)?;
    Ok(())
  }

  /// Ensure that dnsmasq as a minimal config
  #[inline]
  pub(crate) fn ensure(&self) -> Result<(), ErrorHint> {
    println!(
      "[INFO] Ensuring a minimal dnsmasq config inside {}",
      &self.config_dir
    );
    self.gen_main_conf()?;
    self.set_dns()?;
    fs::create_dir_all(format!("{}/dnsmasq.d", &self.config_dir)).map_err(
      |err| {
        ErrorHint::Error(format!(
          "unable to create dnsmasq.d directory got error: {err}"
        ))
      },
    )?;
    println!("[INFO] Minimal dnsmasq config is ensured");
    Ok(())
  }

  /// Set dns server address to resolve domain name if not existing in local
  #[inline]
  pub(crate) fn set_dns(&self) -> Result<(), ErrorHint> {
    let data = match self.read_main_conf() {
      Err(_err) => {
        self.gen_main_conf()?;
        self.read_main_conf()?
      }
      Ok(data) => data,
    };
    let lines = data.lines();
    let mut new_data = String::new();
    for line in lines {
      if line.starts_with("server=") {
        continue;
      }
      new_data.push_str(line);
    }
    for dns in &self.dns {
      new_data.push_str(format!("server={dns}\n").as_str());
    }
    self.write_main_conf(&new_data)?;
    Ok(())
  }

  /// Generate domain records file for dnsmasq
  #[inline]
  pub(crate) fn generate_domains_file(
    &self,
    name: &str,
    domains: &[(String, String)],
  ) -> Result<(), ErrorHint> {
    let mut data = String::new();
    for domain in domains {
      let (name, ip) = domain;
      data.push_str(format!("address=/{name}/{ip}\n").as_str());
    }
    let file_path = format!("{}/dnsmasq.d/{name}.conf", &self.config_dir);
    fs::write(file_path, data).map_err(|err| {
      ErrorHint::Warning(format!(
        "unable to create domains file for {name} got error: {err}"
      ))
    })?;
    Ok(())
  }

  /// Remove domain records file for dnsmasq
  pub(crate) fn remove_domains_file(
    &self,
    name: &str,
  ) -> Result<(), ErrorHint> {
    let file_path = format!("{}/dnsmasq.d/{name}.conf", &self.config_dir);
    fs::remove_file(file_path).map_err(|err| {
      ErrorHint::Warning(format!(
        "unable to remove domains file for {name} got error: {err}"
      ))
    })?;
    Ok(())
  }

  /// Clear all domain records file for dnsmasq
  pub(crate) fn clear_domains(&self) -> Result<(), ErrorHint> {
    let domain_dir = format!("{}/dnsmasq.d", &self.config_dir);
    fs::remove_dir_all(&domain_dir).map_err(|err| {
      ErrorHint::Warning(format!(
        "unable to remove domains directory got error: {err}"
      ))
    })?;
    fs::create_dir_all(&domain_dir).map_err(|err| {
      ErrorHint::Warning(format!(
        "unable to create domains directory got error: {err}"
      ))
    })?;
    Ok(())
  }
}

/// Create a new dnsmasq instance
pub(crate) fn new(config_path: &str) -> Dnsmasq {
  Dnsmasq::new(config_path)
}
