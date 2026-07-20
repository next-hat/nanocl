use std::{collections::BTreeMap, io::ErrorKind, str::FromStr};

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::stubs::proxy::ProxyRule;

/// Kind of rule configuration:
/// * Site for HTTP/HTTPS
/// * Stream for TCP/UDP
#[derive(Clone, Copy, Debug)]
pub enum NginxRuleKind {
  Site,
  Stream,
}

/// All generated nginx files before a candidate configuration mutation.
///
/// Upstream files can be rewritten while rendering a rule, so a transaction
/// snapshots the complete managed directories rather than only the rule's
/// top-level site and stream files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreConfigSnapshot {
  pub sites: BTreeMap<String, String>,
  pub streams: BTreeMap<String, String>,
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
    let temp_path = format!("{}.tmp", path.0);
    tokio::fs::write(&temp_path, data).await.map_err(|err| {
      err.map_err_context(|| format!("Unable to create {} file", path.0))
    })?;
    tokio::fs::rename(&temp_path, &path.0)
      .await
      .map_err(|err| {
        err.map_err_context(|| format!("Unable to replace {} file", path.0))
      })?;
    match tokio::fs::symlink(&path.0, &path.1).await {
      Ok(()) => {}
      Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
      Err(err) => {
        return Err(
          *err
            .map_err_context(|| format!("Unable to create {} symlink", path.1)),
        );
      }
    }
    Ok(())
  }

  pub async fn delete_conf_file(
    &self,
    name: &str,
    kind: &NginxRuleKind,
  ) -> IoResult<()> {
    let path = self.gen_path(name, kind);
    for path in [path.1, path.0] {
      match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
          return Err(
            *err.map_err_context(|| format!("Unable to delete {path}")),
          );
        }
      }
    }
    Ok(())
  }

  async fn replace_conf_file(
    &self,
    name: &str,
    data: Option<&str>,
    kind: &NginxRuleKind,
  ) -> IoResult<()> {
    match data {
      Some(data) => self.write_conf_file(name, data, kind).await,
      None => self.delete_conf_file(name, kind).await,
    }
  }

  pub async fn replace_rule(
    &self,
    name: &str,
    site: Option<&str>,
    stream: Option<&str>,
  ) -> IoResult<()> {
    self
      .replace_conf_file(name, site, &NginxRuleKind::Site)
      .await?;
    self
      .replace_conf_file(name, stream, &NginxRuleKind::Stream)
      .await
  }

  async fn read_config_dir(
    &self,
    name: &str,
  ) -> IoResult<BTreeMap<String, String>> {
    let dir = format!("{}/{name}-available", self.dir);
    let mut entries = tokio::fs::read_dir(&dir)
      .await
      .map_err(|err| err.map_err_context(|| format!("Unable to read {dir}")))?;
    let mut files = BTreeMap::new();
    while let Some(entry) = entries.next_entry().await.map_err(|err| {
      err.map_err_context(|| format!("Unable to read an entry from {dir}"))
    })? {
      let file_type = entry.file_type().await.map_err(|err| {
        err.map_err_context(|| {
          format!("Unable to inspect {}", entry.path().display())
        })
      })?;
      if !file_type.is_file() {
        continue;
      }
      let file_name = entry.file_name().to_string_lossy().into_owned();
      let data =
        tokio::fs::read_to_string(entry.path())
          .await
          .map_err(|err| {
            err.map_err_context(|| format!("Unable to read {dir}/{file_name}"))
          })?;
      files.insert(file_name, data);
    }
    Ok(files)
  }

  async fn clear_config_dir(&self, path: &str) -> IoResult<()> {
    let mut entries = tokio::fs::read_dir(path).await.map_err(|err| {
      err.map_err_context(|| format!("Unable to read {path}"))
    })?;
    while let Some(entry) = entries.next_entry().await.map_err(|err| {
      err.map_err_context(|| format!("Unable to read an entry from {path}"))
    })? {
      let entry_path = entry.path();
      tokio::fs::remove_file(&entry_path).await.map_err(|err| {
        err.map_err_context(|| {
          format!("Unable to delete {}", entry_path.display())
        })
      })?;
    }
    Ok(())
  }

  async fn restore_config_dir(
    &self,
    name: &str,
    files: &BTreeMap<String, String>,
  ) -> IoResult<()> {
    let available = format!("{}/{name}-available", self.dir);
    let enabled = format!("{}/{name}-enabled", self.dir);
    self.clear_config_dir(&enabled).await?;
    self.clear_config_dir(&available).await?;
    for (file_name, data) in files {
      let source = format!("{available}/{file_name}");
      let target = format!("{enabled}/{file_name}");
      tokio::fs::write(&source, data).await.map_err(|err| {
        err.map_err_context(|| format!("Unable to restore {source}"))
      })?;
      tokio::fs::symlink(&source, &target).await.map_err(|err| {
        err.map_err_context(|| format!("Unable to restore {target}"))
      })?;
    }
    Ok(())
  }

  pub async fn snapshot_config(&self) -> IoResult<StoreConfigSnapshot> {
    Ok(StoreConfigSnapshot {
      sites: self.read_config_dir("sites").await?,
      streams: self.read_config_dir("streams").await?,
    })
  }

  pub async fn clear_config(&self) -> IoResult<()> {
    for name in ["sites", "streams"] {
      self
        .clear_config_dir(&format!("{}/{name}-enabled", self.dir))
        .await?;
      self
        .clear_config_dir(&format!("{}/{name}-available", self.dir))
        .await?;
    }
    Ok(())
  }

  pub async fn restore_config(
    &self,
    snapshot: &StoreConfigSnapshot,
  ) -> IoResult<()> {
    self.restore_config_dir("sites", &snapshot.sites).await?;
    self.restore_config_dir("streams", &snapshot.streams).await
  }
}
