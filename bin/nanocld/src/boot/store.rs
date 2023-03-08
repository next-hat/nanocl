use std::collections::HashMap;
use std::time;
use std::thread;
use std::path::Path;

use bollard_next::container::CreateContainerOptions;
use bollard_next::service::{HostConfig, RestartPolicy, RestartPolicyNameEnum};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use nanocl_stubs::cargo_config::CargoConfigPartial;
use nanocl_stubs::cargo_config::ContainerConfig;
use nanocl_stubs::config::DaemonConfig;

use crate::utils;
use crate::models::Pool;
use crate::error::CliError;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

/// Generate store host config
/// Generate a host config struct for the store container
fn gen_store_host_conf(config: &DaemonConfig) -> HostConfig {
  let path = Path::new(&config.state_dir).join("store/data");

  let binds = vec![format!("{}:/cockroach/cockroach-data", path.display())];

  HostConfig {
    binds: Some(binds),
    restart_policy: Some(RestartPolicy {
      name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
      maximum_retry_count: None,
    }),
    network_mode: Some(String::from("system")),
    ..Default::default()
  }
}

/// Generate a cargo config for the store
/// The store is a cockroachdb instance
/// It will generate a cargo for our store to register it in the system namespace
fn gen_store_cargo_conf(
  name: &str,
  config: &DaemonConfig,
) -> CargoConfigPartial {
  let mut labels = HashMap::new();
  labels.insert("io.nanocl.cargo".into(), name.into());
  labels.insert("io.nanocl.namespace".into(), "system".into());
  let host_config = Some(gen_store_host_conf(config));
  CargoConfigPartial {
    name: name.into(),
    replication: None,
    container: ContainerConfig {
      image: Some("cockroachdb/cockroach:v22.2.5".into()),
      labels: Some(labels.to_owned()),
      host_config,
      cmd: Some(vec!["start-single-node".into(), "--insecure".into()]),
      ..Default::default()
    },
  }
}

/// Ensure store is running
/// Verify is store is running and boot it if not
async fn boot(
  config: &DaemonConfig,
  docker_api: &bollard_next::Docker,
) -> Result<(), bollard_next::errors::Error> {
  let container_name = "store.system";

  if docker_api
    .inspect_container(container_name, None)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let options = Some(CreateContainerOptions {
    name: container_name,
    ..Default::default()
  });
  let config = gen_store_cargo_conf(container_name, config);
  let container = docker_api
    .create_container(options, config.container)
    .await?;
  docker_api
    .start_container::<String>(&container.id, None)
    .await?;
  Ok(())
}

/// Ensure existance of a container for our store
/// we use cockroachdb with a postgresql connector.
/// we also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
pub(crate) async fn ensure(
  config: &DaemonConfig,
  docker_api: &bollard_next::Docker,
) -> Result<Pool, CliError> {
  log::info!("Booting store");
  boot(config, docker_api).await?;
  // We wait 500ms to ensure store is booted
  // It's a tricky hack to avoid some error printed by postgresql connector for now.
  thread::sleep(time::Duration::from_millis(500));
  let postgres_ip = utils::store::get_store_ip_addr(docker_api).await?;
  log::info!("Connecting to store");
  // Connect to postgresql
  let pool = utils::store::create_pool(postgres_ip).await;
  let mut conn = utils::store::get_pool_conn(&pool)?;
  log::info!("Store connected");
  // This will run the necessary migrations.
  // See the documentation for `MigrationHarness` for
  // all available methods.
  log::info!("Running migrations");
  conn.run_pending_migrations(MIGRATIONS).map_err(|err| {
    CliError::new(1, format!("Failed to run sql migrations: {}", err))
  })?;
  Ok(pool)
}
