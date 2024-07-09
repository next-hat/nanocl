use std::str::FromStr;

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::stubs::proxy::ProxyRule;

/// Kind of rule configuration:
/// * Site for HTTP/HTTPS
/// * Stream for TCP/UDP
#[derive(Debug)]
pub enum NginxRuleKind {
  Site,
  Stream,
}

/// Implement Display for RuleKind for better display message
impl std::fmt::Display for NginxRuleKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Site => write!(f, "Site"),
      Self::Stream => write!(f, "Stream"),
    }
  }
}

/// Implement From<ProxyRule> for RuleKind to convert ProxyRule to RuleKind
impl From<ProxyRule> for NginxRuleKind {
  fn from(rule: ProxyRule) -> Self {
    match rule {
      ProxyRule::Http(_) => Self::Site,
      ProxyRule::Stream(_) => Self::Stream,
    }
  }
}

impl FromStr for NginxRuleKind {
  type Err = IoError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "Site" | "site" => Ok(Self::Site),
      "Stream" | "stream" => Ok(Self::Stream),
      _ => Err(IoError::invalid_data(
        format!("Invalid RuleKind: {s}").as_str(),
        "expected | Site | site | Stream | stream",
      )),
    }
  }
}

#[derive(Clone)]
pub struct Store {
  pub dir: String,
}

impl Store {
  pub fn new(dir: &str) -> Self {
    Self {
      dir: dir.to_owned(),
    }
  }

  fn gen_path(&self, name: &str, kind: &NginxRuleKind) -> (String, String) {
    let dir = &self.dir;
    match kind {
      NginxRuleKind::Site => (
        format!("{dir}/sites-available/{name}.conf"),
        format!("{dir}/sites-enabled/{name}.conf"),
      ),
      NginxRuleKind::Stream => (
        format!("{dir}/streams-available/{name}.conf"),
        format!("{dir}/streams-enabled/{name}.conf"),
      ),
    }
  }

  pub async fn write_conf_file(
    &self,
    name: &str,
    data: &str,
    kind: &NginxRuleKind,
  ) -> IoResult<()> {
    let path = self.gen_path(name, kind);
    tokio::fs::write(&path.0, data).await.map_err(|err| {
      err.map_err_context(|| format!("Unable to create {} file", path.0))
    })?;
    let _ = tokio::fs::symlink(&path.0, &path.1).await.map_err(|err| {
      err.map_err_context(|| format!("Unable to create {} symlink", path.1))
    });
    Ok(())
  }

  pub async fn delete_conf_file(&self, name: &str, kind: &NginxRuleKind) {
    let path = self.gen_path(name, kind);
    let _ = tokio::fs::remove_file(&path.0).await;
    let _ = tokio::fs::remove_file(&path.1).await;
  }
}
