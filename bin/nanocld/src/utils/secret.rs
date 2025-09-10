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

/// Transform and optional vector of secrets to a vector of envs from the database
///
pub async fn load_env_secrets(
  secrets: &Option<Vec<String>>,
  state: &SystemState,
) -> IoResult<Vec<String>> {
  let mut env_secrets: Vec<String> = Vec::new();
  if let Some(secrets) = &secrets {
    let filter = GenericFilter::new()
      .r#where("key", GenericClause::In(secrets.clone()))
      .r#where("kind", GenericClause::Eq("nanocl.io/env".to_owned()));
    let secrets = SecretDb::transform_read_by(&filter, &state.inner.pool)
      .await?
      .into_iter()
      .map(|secret| {
        let envs = serde_json::from_value::<Vec<String>>(secret.data)?;
        Ok::<_, IoError>(envs)
      })
      .collect::<IoResult<Vec<Vec<String>>>>()?;
    // Flatten the secrets to have envs in a single vector
    env_secrets = secrets.into_iter().flatten().collect();
  }
  Ok(env_secrets)
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
  if let Some(secrets) = &secrets {
    let filter = GenericFilter::new()
      .r#where("key", GenericClause::In(secrets.clone()))
      .r#where("kind", GenericClause::Eq("nanocl.io/tls".to_owned()));
    let secrets =
      SecretDb::transform_read_by(&filter, &state.inner.pool).await?;
    secrets
      .into_iter()
      .map(|secret| {
        let secrets_dir = secret_dir.clone();
        async move {
          let tls = serde_json::from_value::<ProxySslConfig>(secret.data)?;
          fs::write(
            format!("{secrets_dir}/{}.crt", secret.name),
            tls.certificate,
          )
          .await?;
          fs::write(
            format!("{secrets_dir}/{}.key", secret.name),
            tls.certificate_key,
          )
          .await?;
          if let Some(certificate_client) = tls.certificate_client {
            fs::write(
              format!("{secrets_dir}/{}.ca", secret.name),
              certificate_client,
            )
            .await?;
          }
          Ok::<_, IoError>(())
        }
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<_>>()
      .await;
  }
  Ok(secret_dir)
}
