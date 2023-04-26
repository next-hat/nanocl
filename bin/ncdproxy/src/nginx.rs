use std::fs;
use std::str::FromStr;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use nanocld_client::stubs::proxy::ProxyRule;

#[derive(Debug)]
pub enum NginxConfKind {
  Site,
  Stream,
}

impl std::fmt::Display for NginxConfKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Site => write!(f, "Site"),
      Self::Stream => write!(f, "Stream"),
    }
  }
}

impl From<ProxyRule> for NginxConfKind {
  fn from(rule: ProxyRule) -> Self {
    match rule {
      ProxyRule::Http(_) => Self::Site,
      ProxyRule::Stream(_) => Self::Stream,
    }
  }
}

impl FromStr for NginxConfKind {
  type Err = IoError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "Site" => Ok(Self::Site),
      "Stream" => Ok(Self::Stream),
      "site" => Ok(Self::Site),
      "stream" => Ok(Self::Stream),
      _ => Err(IoError::new(
        format!("Invalid NginxConfKind: {s}"),
        std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          "expected | Site | site | Stream | stream",
        ),
      )),
    }
  }
}

#[derive(Clone, Debug)]
pub struct Nginx {
  pub conf_dir: String,
}

impl Nginx {
  pub fn new(conf_dir: &str) -> Self {
    Self {
      conf_dir: conf_dir.to_owned(),
    }
  }

  #[inline]
  fn gen_conf_path(&self, name: &str, kind: &NginxConfKind) -> String {
    match kind {
      NginxConfKind::Site => {
        format!("{}/sites-enabled/{name}.conf", &self.conf_dir)
      }
      NginxConfKind::Stream => {
        format!("{}/streams-enabled/{name}.conf", &self.conf_dir)
      }
    }
  }

  #[inline]
  pub fn ensure(&self) -> IoResult<()> {
    // Ensure sites-enabled directory exists
    let sites_enabled_dir = format!("{}/sites-enabled", self.conf_dir);
    fs::create_dir_all(&sites_enabled_dir).map_err(|err| {
      err.map_err_context(|| {
        format!("Unable to create directory {sites_enabled_dir}")
      })
    })?;
    // Ensure streams-enabled directory exists
    let streams_enabled_dir = format!("{}/streams-enabled", self.conf_dir);
    fs::create_dir_all(&streams_enabled_dir).map_err(|err| {
      err.map_err_context(|| {
        format!("Cannot create directory {streams_enabled_dir}")
      })
    })?;
    // Ensure conf.d directory exists
    let conf_d = format!("{}/conf.d", self.conf_dir);
    fs::create_dir_all(conf_d).map_err(|err| {
      err.map_err_context(|| {
        format!("Cannot create directory {streams_enabled_dir}")
      })
    })?;
    Ok(())
  }

  pub fn write_default_conf(&self) -> IoResult<()> {
    let default_conf = "server {
  listen 80 default_server;
  listen [::]:80 default_server ipv6only=on;
  server_name _ default_server;

  root /usr/share/nginx/html;
  try_files $uri $uri/ /index.html;
  error_page 502 /502.html;
  error_page 403 /403.html;
}"
    .to_string();

    let path = format!("{}/conf.d/default.conf", self.conf_dir);

    fs::write(&path, &default_conf).map_err(|err| {
      err.map_err_context(|| format!("Unable to create {path} file"))
    })?;

    log::debug!("Writing default file conf:\n {default_conf}");

    Ok(())
  }

  #[inline]
  pub fn write_conf_file(
    &self,
    name: &str,
    data: &str,
    kind: &NginxConfKind,
  ) -> IoResult<()> {
    let path = self.gen_conf_path(name, kind);
    fs::write(&path, data).map_err(|err| {
      err.map_err_context(|| format!("Unable to create {path} file"))
    })?;
    Ok(())
  }

  #[inline]
  pub async fn delete_conf_file(&self, name: &str) {
    let path = self.gen_conf_path(name, &NginxConfKind::Site);
    let _ = tokio::fs::remove_file(&path).await;
    let path = self.gen_conf_path(name, &NginxConfKind::Stream);
    let _ = tokio::fs::remove_file(&path).await;
  }

  // TODO: Uncommand to enable sync resources
  // #[inline]
  // pub fn clear_conf(&self) -> IoResult<()> {
  //   let sites_enabled_dir = format!("{}/sites-enabled", self.conf_dir);
  //   fs::remove_dir_all(&sites_enabled_dir).map_err(|err| {
  //     err.map_err_context(|| {
  //       format!("Cannot remove directory {sites_enabled_dir}")
  //     })
  //   })?;
  //   let streams_enabled_dir = format!("{}/streams-enabled", self.conf_dir);
  //   fs::remove_dir_all(&streams_enabled_dir).map_err(|err| {
  //     err.map_err_context(|| {
  //       format!("Cannot remove directory {streams_enabled_dir}")
  //     })
  //   })?;
  //   fs::create_dir_all(&sites_enabled_dir).map_err(|err| {
  //     err.map_err_context(|| {
  //       format!("Cannot create directory {sites_enabled_dir}")
  //     })
  //   })?;
  //   fs::create_dir_all(&streams_enabled_dir).map_err(|err| {
  //     err.map_err_context(|| {
  //       format!("Cannot create directory {streams_enabled_dir}")
  //     })
  //   })?;
  //   Ok(())
  // }
}
