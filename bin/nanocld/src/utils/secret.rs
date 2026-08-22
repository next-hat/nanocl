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

fn validate_secret_file_stem(name: &str) -> IoResult<()> {
  if name.is_empty()
    || matches!(name, "." | "..")
    || name.contains(['/', '\\', '\0'])
  {
    return Err(IoError::invalid_data(
      "Secret",
      &format!("Secret name {name:?} is not a safe mounted filename"),
    ));
  }
  Ok(())
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
        validate_secret_file_stem(&secret.name)?;
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
    format!("{}/secrets/{}/{}", state.inner.config.state_dir, kind, key);
  fs::create_dir_all(&secret_dir).await.map_err(|error| {
    IoError::interrupted(
      "CreateTlsSecrets",
      &format!("Unable to create {secret_dir}: {error}"),
    )
  })?;
  fs::set_permissions(&secret_dir, std::fs::Permissions::from_mode(0o700))
    .await?;
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
  fn mounted_secret_names_preserve_safe_basenames_and_reject_traversal() {
    for safe in ["api", "api.tls", "api-key_1"] {
      assert!(validate_secret_file_stem(safe).is_ok());
    }
    for unsafe_name in ["", ".", "..", "../api", "dir/api", "dir\\api"] {
      let error = validate_secret_file_stem(unsafe_name).unwrap_err();
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
