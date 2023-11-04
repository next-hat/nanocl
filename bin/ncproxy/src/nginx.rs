use std::str::FromStr;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use nanocl_error::io::{IoError, FromIo, IoResult};

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

  fn gen_conf_path(
    &self,
    name: &str,
    kind: &NginxConfKind,
  ) -> (String, String) {
    match kind {
      NginxConfKind::Site => (
        format!("{}/sites-available/{name}.conf", &self.conf_dir),
        format!("{}/sites-enabled/{name}.conf", &self.conf_dir),
      ),
      NginxConfKind::Stream => (
        format!("{}/streams-available/{name}.conf", &self.conf_dir),
        format!("{}/streams-enabled/{name}.conf", &self.conf_dir),
      ),
    }
  }

  async fn ensure_dir(&self, name: &str) -> IoResult<()> {
    let path = format!("{}/{name}", &self.conf_dir);
    tokio::fs::create_dir_all(&path).await.map_err(|err| {
      err.map_err_context(|| format!("Unable to create {path} directory"))
    })?;
    Ok(())
  }

  /// ## Ensure
  ///
  /// Ensure default configuration files and directories
  ///
  pub async fn ensure(&self) -> IoResult<()> {
    [
      "sites-available",
      "sites-enabled",
      "streams-available",
      "streams-enabled",
    ]
    .into_iter()
    .map(|name| self.ensure_dir(name))
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<IoResult<()>>>()
    .await
    .into_iter()
    .collect::<IoResult<()>>()?;
    self.ensure_default_conf().await?;
    Ok(())
  }

  async fn ensure_default_conf(&self) -> IoResult<()> {
    let default_conf = "server {
  listen 80 default_server;
  listen [::]:80 ipv6only=on default_server;
  server_name _;

  root /usr/share/nginx/html;
  try_files $uri $uri/ /index.html;
  error_page 502 /502.html;
  error_page 403 /403.html;
}"
    .to_owned();
    let path = format!("{}/sites-available/default", self.conf_dir);
    tokio::fs::write(&path, &default_conf)
      .await
      .map_err(|err| {
        err.map_err_context(|| format!("Unable to create {path} file"))
      })?;
    let _ = tokio::fs::symlink(
      &path,
      format!("{}/sites-enabled/default", self.conf_dir),
    )
    .await;
    log::debug!("Writing default file conf:\n {default_conf}");
    Ok(())
  }

  pub async fn write_conf_file(
    &self,
    name: &str,
    data: &str,
    kind: &NginxConfKind,
  ) -> IoResult<()> {
    let path = self.gen_conf_path(name, kind);
    tokio::fs::write(&path.0, data).await.map_err(|err| {
      err.map_err_context(|| format!("Unable to create {} file", path.0))
    })?;
    let _ = tokio::fs::symlink(&path.0, &path.1).await.map_err(|err| {
      err.map_err_context(|| format!("Unable to create {} symlink", path.1))
    });
    Ok(())
  }

  pub async fn delete_conf_file(&self, name: &str) {
    let path = self.gen_conf_path(name, &NginxConfKind::Site);
    let _ = tokio::fs::remove_file(&path.1).await;
    let path = self.gen_conf_path(name, &NginxConfKind::Stream);
    let _ = tokio::fs::remove_file(&path.1).await;
  }

  // TODO: Uncommand to enable sync resources
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
