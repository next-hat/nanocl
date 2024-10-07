use bollard_next::{
  container::{
    RemoveContainerOptions, RenameContainerOptions, StartContainerOptions,
    StopContainerOptions, WaitContainerOptions,
  },
  secret::{HostConfig, RestartPolicy, RestartPolicyNameEnum},
};
use futures::{stream::FuturesUnordered, StreamExt};
use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  cargo::Cargo,
  cargo_spec::ReplicationMode,
  generic::{GenericClause, GenericFilter},
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
};
use ntex::rt;

use crate::{
  models::{CargoDb, ObjPsStatusDb, ProcessDb, SecretDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Function that execute the init container before the main cargo container
///
async fn init_container(cargo: &Cargo, state: &SystemState) -> IoResult<()> {
  match cargo.spec.init_container.clone() {
    Some(mut before) => {
      let image = before
        .image
        .clone()
        .unwrap_or(cargo.spec.container.image.clone().unwrap());
      before.image = Some(image.clone());
      before.host_config = Some(HostConfig {
        network_mode: Some("nanoclbr0".to_owned()),
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
        .await
        .map_err(|err| err.map_err_context(|| "InitContainer"))?;
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
                Some(error) => {
                  error.message.unwrap_or("Unknown error".to_owned())
                }
                None => "Unknown error".to_owned(),
              };
              return Err(IoError::interrupted(
                "Cargo",
                &format!("Error while waiting for init container: {error}"),
              ));
            }
          }
          Err(err) => {
            return Err(IoError::interrupted(
              "Cargo",
              &format!("Error while waiting for init container: {err}"),
            ));
          }
        }
      }
      Ok(())
    }
    None => Ok(()),
  }
}

/// Execute the cargo spec to create the cargo container
/// If the cargo has a init container, it will be executed before the main cargo container
///
pub async fn create(
  cargo: &Cargo,
  number: usize,
  state: &SystemState,
) -> IoResult<Vec<Process>> {
  init_container(cargo, state).await?;
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
        let name = format!(
          "{}-{}.{}.c",
          cargo.spec.name, short_id, cargo.namespace_name
        );
        let spec = cargo.spec.clone();
        let container = spec.container;
        let host_config = container.host_config.unwrap_or_default();
        // Add cargo label to the container to track it
        let mut labels = container.labels.to_owned().unwrap_or_default();
        labels
          .insert("io.nanocl.c".to_owned(), cargo.spec.cargo_key.to_owned());
        labels
          .insert("io.nanocl.n".to_owned(), cargo.namespace_name.to_owned());
        labels.insert(
          "com.docker.compose.project".into(),
          format!("nanocl_{}", cargo.namespace_name),
        );
        let auto_remove = host_config.auto_remove.unwrap_or(false);
        if auto_remove {
          return Err(IoError::interrupted(
            "CargoCreate",
            "Auto remove is not allowed for cargo use a job instead",
          ));
        }
        let restart_policy =
          Some(host_config.restart_policy.unwrap_or(RestartPolicy {
            name: Some(RestartPolicyNameEnum::ALWAYS),
            maximum_retry_count: None,
          }));
        let mut env = container.env.unwrap_or_default();
        // merge cargo env with secret env
        env.extend(secret_envs);
        env.push(format!("NANOCL_NODE={}", state.inner.config.hostname));
        env.push(format!("NANOCL_NODE_ADDR={}", state.inner.config.gateway));
        env.push(format!(
          "NANOCL_CARGO_KEY={}",
          cargo.spec.cargo_key.to_owned()
        ));
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
            network_mode: Some("nanoclbr0".to_owned()),
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
        )
        .await
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<IoResult<Process>>>()
    .await
    .into_iter()
    .collect::<IoResult<Vec<Process>>>()
}

/// Start cargo instances
///
pub async fn start(key: &str, state: &SystemState) -> IoResult<()> {
  let cargo = CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  let processes =
    ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.inner.pool)
      .await?;
  // TODO: FIND BEST NODES TO RUN WORKLOAD
  // let nodes =
  //   MetricDb::find_best_nodes(90.0, 90.0, 100, &state.inner.pool).await?;
  // log::debug!("BEST NODES FOR CARGO {key}: {nodes:?}");
  if processes.is_empty() {
    let number = match &cargo.spec.replication {
      Some(ReplicationMode::Static(replication)) => replication.number,
      _ => 1,
    };
    create(&cargo, number, state).await?;
  }
  super::process::start_instances(
    &cargo.spec.cargo_key,
    &ProcessKind::Cargo,
    state,
  )
  .await?;
  Ok(())
}

/// Function that update the cargo container by creating new instances before removing the old ones
/// This way we can have zero downtime deployment
///
pub async fn update(key: &str, state: &SystemState) -> IoResult<()> {
  let cargo = CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  let processes = ProcessDb::read_by_kind_key(key, &state.inner.pool).await?;
  // rename old instances to flag them for deletion
  processes
    .iter()
    .map(|process| {
      let docker_api = state.inner.docker_api.clone();
      async move {
        if process
          .data
          .state
          .clone()
          .unwrap_or_default()
          .restarting
          .unwrap_or_default()
        {
          docker_api
            .stop_container(&process.name, None::<StopContainerOptions>)
            .await
            .map_err(|err| err.map_err_context(|| "StopContainer"))?;
        }
        let new_name = format!("tmp-{}", process.name);
        docker_api
          .rename_container(
            &process.key,
            RenameContainerOptions { name: &new_name },
          )
          .await
          .map_err(|err| err.map_err_context(|| "RenameContainer"))?;
        Ok::<_, IoError>(())
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<IoResult<Vec<_>>>()?;
  let number = match &cargo.spec.replication {
    Some(ReplicationMode::Static(replication)) => replication.number,
    _ => 1,
  };
  // Create instance with the new spec
  let new_instances = match create(&cargo, number, state).await {
    Err(err) => {
      log::error!(
        "Unable to create cargo instance {} : {err}",
        cargo.spec.cargo_key
      );
      return Err(err);
    }
    Ok(instances) => instances,
  };
  // start created containers
  match super::process::start_instances(key, &ProcessKind::Cargo, state).await {
    Err(err) => {
      log::error!(
        "Unable to start cargo instance {} : {err}",
        cargo.spec.cargo_key
      );
      let state_ptr_ptr = state.clone();
      let _ = super::process::delete_instances(
        &new_instances
          .iter()
          .map(|p| p.key.clone())
          .collect::<Vec<_>>(),
        &state_ptr_ptr,
      )
      .await;
      let res = processes
        .iter()
        .map(|process| {
          let docker_api = state_ptr_ptr.inner.docker_api.clone();
          async move {
            docker_api
              .rename_container(
                &process.key,
                RenameContainerOptions {
                  name: &process.name,
                },
              )
              .await
              .map_err(|err| err.map_err_context(|| "RenameContainer"))?;
            Ok::<_, IoError>(())
          }
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<IoResult<Vec<_>>>();
      if let Err(err) = res {
        log::error!("Unable to rename containers back: {err}");
      }
    }
    Ok(_) => {
      log::debug!("cargo instance {} started", cargo.spec.cargo_key);
      // Delete old containers
      let state_ptr_ptr = state.clone();
      rt::spawn(async move {
        ntex::time::sleep(std::time::Duration::from_secs(4)).await;
        let _ = super::process::delete_instances(
          &processes.iter().map(|p| p.key.clone()).collect::<Vec<_>>(),
          &state_ptr_ptr,
        )
        .await;
      });
    }
  }
  ObjPsStatusDb::update_actual_status(
    key,
    &ObjPsStatusKind::Start,
    &state.inner.pool,
  )
  .await?;
  state
    .emit_normal_native_action_sync(&cargo, NativeEventAction::Start)
    .await;
  Ok(())
}

/// Delete cargo instances and the cargo itself in the database
///
pub async fn delete(key: &str, state: &SystemState) -> IoResult<()> {
  let processes = ProcessDb::read_by_kind_key(key, &state.inner.pool).await?;
  for process in processes {
    let _ = state
      .inner
      .docker_api
      .stop_container(&process.key, None::<StopContainerOptions>)
      .await;
    let _ = state
      .inner
      .docker_api
      .remove_container(&process.key, None::<RemoveContainerOptions>)
      .await;
  }
  let cargo = CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  CargoDb::clear_by_pk(key, &state.inner.pool).await?;
  state
    .emit_normal_native_action_sync(&cargo, NativeEventAction::Destroy)
    .await;
  Ok(())
}
