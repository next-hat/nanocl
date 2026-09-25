use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;

use futures::{StreamExt, stream::FuturesUnordered};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  process::ProcessKind,
  proxy::ProxySslConfig,
};
use tokio::fs;

use crate::{
  models::{SecretDb, SystemState},
  repositories::generic::*,
};

fn validate_secret_path_component(name: &str) -> IoResult<()> {
  if name.is_empty()
    || matches!(name, "." | "..")
    || name.contains(['/', '\\', '\0'])
  {
    return Err(IoError::invalid_data(
      "Secret",
      &format!("Secret path component {name:?} is not a safe filename"),
    ));
  }
  Ok(())
}

/// Validate before any filesystem mutation, then check each directory before
/// descending so an existing directory symlink cannot redirect creation or chmod.
async fn prepare_secret_directory(
  state_dir: &str,
  kind: &ProcessKind,
  key: &str,
) -> IoResult<String> {
  validate_secret_path_component(key)?;
  fs::create_dir_all(state_dir).await?;
  let mut parent = fs::canonicalize(state_dir).await?;
  let kind = kind.to_string();
  for component in ["secrets", kind.as_str(), key] {
    let directory = parent.join(component);
    match fs::create_dir(&directory).await {
      Ok(()) => {}
      Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
      Err(error) => {
        return Err(IoError::interrupted(
          "CreateTlsSecrets",
          &format!("Unable to create {}: {error}", directory.display()),
        ));
      }
    }
    let canonical = fs::canonicalize(&directory).await?;
    if canonical != directory || !fs::metadata(&canonical).await?.is_dir() {
      return Err(IoError::invalid_data(
        "CreateTlsSecrets",
        "Secret path must be a directory without symlink redirection",
      ));
    }
    parent = canonical;
  }
  let directory = parent.into_os_string().into_string().map_err(|_| {
    IoError::invalid_data("CreateTlsSecrets", "Secret directory is not UTF-8")
  })?;
  fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
    .await?;
  Ok(directory)
}

async fn write_secret_file(
  path: String,
  contents: impl AsRef<[u8]>,
) -> IoResult<()> {
  fs::write(&path, contents).await?;
  fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await?;
  Ok(())
}

#[derive(Default)]
struct ResolvedSecrets {
  env: Vec<String>,
  tls: Vec<(String, ProxySslConfig)>,
}

fn env_name(value: &str) -> &str {
  value.split_once('=').map_or(value, |(name, _)| name)
}

fn overlay_env(
  target: &mut Vec<String>,
  positions: &mut HashMap<String, usize>,
  value: String,
) {
  let name = env_name(&value).to_owned();
  if let Some(position) = positions.get(&name).copied() {
    target[position] = value;
  } else {
    positions.insert(name, target.len());
    target.push(value);
  }
}

async fn resolve_secrets(
  secrets: &Option<Vec<String>>,
  state: &SystemState,
) -> IoResult<ResolvedSecrets> {
  let Some(secrets) = secrets else {
    return Ok(ResolvedSecrets::default());
  };
  let mut requested = HashSet::new();
  if let Some(duplicate) = secrets
    .iter()
    .find(|name| !requested.insert((*name).clone()))
  {
    return Err(IoError::invalid_data(
      "Secret",
      &format!("Secret reference {duplicate:?} is duplicated"),
    ));
  }
  let filter =
    GenericFilter::new().r#where("key", GenericClause::In(secrets.clone()));
  let mut found = SecretDb::transform_read_by(&filter, &state.inner.pool)
    .await?
    .into_iter()
    .map(|secret| (secret.name.clone(), secret))
    .collect::<HashMap<_, _>>();
  let mut resolved = ResolvedSecrets::default();
  let mut env_positions = HashMap::new();
  for name in secrets {
    let secret = found.remove(name).ok_or_else(|| {
      IoError::not_found("Secret", &format!("Secret {name:?} does not exist"))
    })?;
    match secret.kind.as_str() {
      "nanocl.io/env" => {
        let values = serde_json::from_value::<Vec<String>>(secret.data)?;
        let mut names = HashSet::new();
        for value in values {
          let variable = env_name(&value);
          if variable.is_empty() || !names.insert(variable.to_owned()) {
            return Err(IoError::invalid_data(
              "Secret",
              &format!(
                "Environment secret {name:?} contains an empty or duplicate variable {variable:?}"
              ),
            ));
          }
          overlay_env(&mut resolved.env, &mut env_positions, value);
        }
      }
      "nanocl.io/tls" => {
        validate_secret_path_component(&secret.name)?;
        resolved.tls.push((
          secret.name,
          serde_json::from_value::<ProxySslConfig>(secret.data)?,
        ));
      }
      kind => {
        return Err(IoError::invalid_data(
          "Secret",
          &format!("Secret {name:?} has unsupported kind {kind:?}"),
        ));
      }
    }
  }
  Ok(resolved)
}

/// Validate that every referenced secret exists, has a supported kind, and can
/// be decoded without materializing files or changing a running container's
/// secret view.
pub async fn validate_tls_secrets(
  secrets: &Option<Vec<String>>,
  state: &SystemState,
) -> IoResult<()> {
  resolve_secrets(secrets, state).await.map(|_| ())
}

/// Transform and optional vector of secrets to a vector of envs from the database
///
pub async fn load_env_secrets(
  secrets: &Option<Vec<String>>,
  state: &SystemState,
) -> IoResult<Vec<String>> {
  Ok(resolve_secrets(secrets, state).await?.env)
}

/// Load tls secrets from the database and create them as file to be mount inside a container
///
pub async fn create_tls_secrets(
  key: &str,
  kind: &ProcessKind,
  secrets: &Option<Vec<String>>,
  state: &SystemState,
) -> IoResult<String> {
  let secret_dir =
    prepare_secret_directory(&state.inner.config.state_dir, kind, key).await?;
  resolve_secrets(secrets, state)
    .await?
    .tls
    .into_iter()
    .map(|(name, tls)| {
      let secrets_dir = secret_dir.clone();
      async move {
        write_secret_file(format!("{secrets_dir}/{name}.crt"), tls.certificate)
          .await?;
        write_secret_file(
          format!("{secrets_dir}/{name}.key"),
          tls.certificate_key,
        )
        .await?;
        if let Some(certificate_client) = tls.certificate_client {
          write_secret_file(
            format!("{secrets_dir}/{name}.ca"),
            certificate_client,
          )
          .await?;
        }
        Ok::<_, IoError>(())
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<IoResult<()>>()?;
  Ok(secret_dir)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn secret_path_components_preserve_safe_names_and_reject_traversal() {
    for safe in ["api", "api.tls", "api-key_1"] {
      assert!(validate_secret_path_component(safe).is_ok());
    }
    for unsafe_name in [
      "",
      ".",
      "..",
      "../api",
      "dir/api",
      "dir\\api",
      "\0",
      "/tmp/api",
      "../../poc_escape",
      "../../store/certs",
      "safe/../escape",
    ] {
      // No async runtime or state directory is needed: unsafe keys must fail
      // before the first filesystem operation.
      let error = futures::executor::block_on(prepare_secret_directory(
        "",
        &ProcessKind::Job,
        unsafe_name,
      ))
      .unwrap_err();
      assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidData);
      assert_eq!(error.context(), Some("Secret"));
    }
  }

  #[test]
  fn environment_overlay_is_ordered_and_later_values_replace_in_place() {
    let mut values = Vec::new();
    let mut positions = HashMap::new();
    for value in ["API_URL=first", "WORKERS=2", "API_URL=second"] {
      overlay_env(&mut values, &mut positions, value.to_owned());
    }
    assert_eq!(values, ["API_URL=second", "WORKERS=2"]);
  }
}
