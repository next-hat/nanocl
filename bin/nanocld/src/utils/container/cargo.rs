use bollard_next::{
  container::{StartContainerOptions, WaitContainerOptions},
  secret::{HostConfig, RestartPolicy, RestartPolicyNameEnum},
};
use futures::{stream::FuturesUnordered, StreamExt};
use nanocl_error::{
  http::{HttpError, HttpResult},
  io::{IoError, IoResult},
};
use nanocl_stubs::{
  cargo::Cargo,
  generic::{GenericClause, GenericFilter},
  process::{Process, ProcessKind},
};

use crate::{
  models::{SecretDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Container to execute before the cargo instances
async fn execute_before(cargo: &Cargo, state: &SystemState) -> HttpResult<()> {
  match cargo.spec.init_container.clone() {
    Some(mut before) => {
      let image = before
        .image
        .clone()
        .unwrap_or(cargo.spec.container.image.clone().unwrap());
      before.image = Some(image.clone());
      before.host_config = Some(HostConfig {
        network_mode: Some(cargo.namespace_name.clone()),
        ..before.host_config.unwrap_or_default()
      });
      super::image::download(
        &image,
        cargo.spec.image_pull_secret.clone(),
        cargo.spec.image_pull_policy.clone().unwrap_or_default(),
        cargo,
        state,
      )
      .await?;
      let mut labels = before.labels.to_owned().unwrap_or_default();
      labels.insert("io.nanocl.c".to_owned(), cargo.spec.cargo_key.to_owned());
      labels.insert("io.nanocl.n".to_owned(), cargo.namespace_name.to_owned());
      labels.insert("io.nanocl.init-c".to_owned(), "true".to_owned());
      labels.insert(
        "com.docker.compose.project".into(),
        format!("nanocl_{}", cargo.namespace_name),
      );
      before.labels = Some(labels);
      let short_id = utils::key::generate_short_id(6);
      let name = format!(
        "init-{}-{}.{}.c",
        cargo.spec.name, short_id, cargo.namespace_name
      );
      super::process::create(
        &ProcessKind::Cargo,
        &name,
        &cargo.spec.cargo_key,
        &before,
        state,
      )
      .await?;
      state
        .inner
        .docker_api
        .start_container(&name, None::<StartContainerOptions<String>>)
        .await?;
      let options = Some(WaitContainerOptions {
        condition: "not-running",
      });
      let mut stream = state.inner.docker_api.wait_container(&name, options);
      while let Some(wait_status) = stream.next().await {
        log::trace!("init_container: wait {wait_status:?}");
        match wait_status {
          Ok(wait_status) => {
            log::debug!("Wait status: {wait_status:?}");
            if wait_status.status_code != 0 {
              let error = match wait_status.error {
                Some(error) => error.message.unwrap_or("Unknown error".into()),
                None => "Unknown error".into(),
              };
              return Err(HttpError::internal_server_error(format!(
                "Error while waiting for before container: {error}"
              )));
            }
          }
          Err(err) => {
            return Err(HttpError::internal_server_error(format!(
              "Error while waiting for before container: {err}"
            )));
          }
        }
      }
      Ok(())
    }
    None => Ok(()),
  }
}

/// Create instances (containers) based on the cargo spec
/// The number of containers created is based on the number of instances defined in the cargo spec
/// Example: cargo-key-(random-id), cargo-key-(random-id1), cargo-key-(random-id2)
pub async fn create(
  cargo: &Cargo,
  number: usize,
  state: &SystemState,
) -> HttpResult<Vec<Process>> {
  execute_before(cargo, state).await?;
  super::image::download(
    &cargo.spec.container.image.clone().unwrap_or_default(),
    cargo.spec.image_pull_secret.clone(),
    cargo.spec.image_pull_policy.clone().unwrap_or_default(),
    cargo,
    state,
  )
  .await?;
  let mut secret_envs: Vec<String> = Vec::new();
  if let Some(secrets) = &cargo.spec.secrets {
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
    secret_envs = secrets.into_iter().flatten().collect();
  }
  (0..number)
    .collect::<Vec<usize>>()
    .into_iter()
    .map(move |current| {
      let secret_envs = secret_envs.clone();
      async move {
        let ordinal_index = if current > 0 {
          current.to_string()
        } else {
          "".to_owned()
        };
        let short_id = utils::key::generate_short_id(6);
        let name = format!("{}-{}.{}.c", cargo.spec.name, short_id, cargo.namespace_name);
        let spec = cargo.spec.clone();
        let container = spec.container;
        let host_config = container.host_config.unwrap_or_default();
        // Add cargo label to the container to track it
        let mut labels = container.labels.to_owned().unwrap_or_default();
        labels.insert("io.nanocl.c".to_owned(), cargo.spec.cargo_key.to_owned());
        labels
          .insert("io.nanocl.n".to_owned(), cargo.namespace_name.to_owned());
        labels.insert(
          "com.docker.compose.project".into(),
          format!("nanocl_{}", cargo.namespace_name),
        );
        let auto_remove =
          host_config
          .auto_remove
          .unwrap_or(false);
        if auto_remove {
          return Err(HttpError::bad_request("Using auto remove for a cargo is not allowed, consider using a job instead"));
        }
        let restart_policy =
          Some(
              host_config
              .restart_policy
              .unwrap_or(RestartPolicy {
                name: Some(RestartPolicyNameEnum::ALWAYS),
                maximum_retry_count: None,
              }),
          );
        let mut env = container.env.unwrap_or_default();
        // merge cargo env with secret env
        env.extend(secret_envs);
        env.push(format!("NANOCL_NODE={}", state.inner.config.hostname));
        env.push(format!("NANOCL_NODE_ADDR={}", state.inner.config.gateway));
        env.push(format!("NANOCL_CARGO_KEY={}", cargo.spec.cargo_key.to_owned()));
        env.push(format!("NANOCL_CARGO_NAMESPACE={}", cargo.namespace_name));
        env.push(format!("NANOCL_CARGO_INSTANCE={}", current));
        // Merge the cargo spec with the container spec
        // And set his network mode to the cargo namespace
        let hostname = match &cargo.spec.container.hostname {
          None => format!("{}{}", ordinal_index, cargo.spec.name),
          Some(hostname) => format!("{}{}", ordinal_index, hostname),
        };
        let new_process = bollard_next::container::Config {
          attach_stderr: Some(true),
          attach_stdout: Some(true),
          tty: Some(true),
          hostname: Some(hostname),
          labels: Some(labels),
          env: Some(env),
          host_config: Some(HostConfig {
            restart_policy,
            ..host_config
          }),
          ..container
        };
        super::process::create(
          &ProcessKind::Cargo,
          &name,
          &cargo.spec.cargo_key,
          &new_process,
          state,
        ).await
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<HttpResult<Process>>>()
    .await
    .into_iter()
    .collect::<HttpResult<Vec<Process>>>()
}
