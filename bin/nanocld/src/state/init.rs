//! File used to describe daemon boot
use std::path::Path;
use std::{time, thread};
use std::collections::HashMap;

use bollard::Docker;
use bollard::container::{
  ListContainersOptions, InspectContainerOptions, NetworkingConfig,
};
use bollard::network::{CreateNetworkOptions, InspectNetworkOptions};

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use nanocl_models::config::DaemonConfig;
use nanocl_models::namespace::NamespacePartial;
use nanocl_models::cargo_config::CargoConfigPartial;

use crate::{utils, repositories};
use crate::models::{Pool, ArgState, DaemonState};

use crate::error::DaemonError;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

/// Ensure existance of the system network that controllers will use.
/// It's ensure existance of a network in your system called `nanoclinternal0`
/// Also registered inside docker as `system-nano-internal0`
async fn ensure_system_network(docker_api: &Docker) -> Result<(), DaemonError> {
  const SYSTEM_NETWORK_KEY: &str = "system-nano-internal0";
  const SYSTEM_NETWORK: &str = "nanoclinternal0";

  // Ensure network existance
  if docker_api
    .inspect_network(SYSTEM_NETWORK_KEY, None::<InspectNetworkOptions<&str>>)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let mut options: HashMap<String, String> = HashMap::new();
  options.insert(
    String::from("com.docker.network.bridge.name"),
    SYSTEM_NETWORK.to_owned(),
  );
  let config = CreateNetworkOptions {
    name: SYSTEM_NETWORK_KEY.to_owned(),
    driver: String::from("bridge"),
    options,
    ..Default::default()
  };
  docker_api.create_network(config).await?;
  Ok(())
}

/// Ensure existance of a container for our store
/// we use cockroachdb with a postgresql connector.
/// we also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
async fn ensure_store(
  config: &DaemonConfig,
  docker_api: &Docker,
) -> Result<Pool, DaemonError> {
  log::info!("Booting store");
  utils::store::boot(config, docker_api).await?;
  // We wait 500ms to ensure store is booted
  // It's a tricky hack to avoid some error printed by postgresql connector for now.
  thread::sleep(time::Duration::from_millis(500));
  let postgres_ip = utils::store::get_store_ip_addr(docker_api).await?;
  log::info!("Connecting to store");
  // Connect to postgresql
  let pool = utils::store::create_pool(postgres_ip.to_owned()).await;
  let mut conn = utils::store::get_pool_conn(&pool)?;
  log::info!("Store connected");
  // This will run the necessary migrations.
  // See the documentation for `MigrationHarness` for
  // all available methods.
  log::info!("Running migrations");
  conn.run_pending_migrations(MIGRATIONS)?;
  Ok(pool)
}

/// Ensure existance of specific namespace in our store.
/// We use it to be sure `system` and `global` namespace exists.
/// system is the namespace where controllers are registered.
/// where global is the namespace used by default.
/// User can registed they own namespace to ensure better encaptusation of projects.
async fn register_namespace(
  name: &str,
  pool: &Pool,
) -> Result<(), DaemonError> {
  match repositories::namespace::find_by_name(name.to_owned(), pool).await {
    Err(_err) => {
      let new_nsp = NamespacePartial {
        name: name.to_owned(),
      };
      repositories::namespace::create(new_nsp, pool).await?;
      Ok(())
    }
    Ok(_) => Ok(()),
  }
}

async fn sync_containers(
  docker_api: &Docker,
  pool: &Pool,
) -> Result<(), DaemonError> {
  log::debug!("Syncing existing container");
  let options = Some(ListContainersOptions::<&str> {
    all: true,
    ..Default::default()
  });
  let containers = docker_api.list_containers(options).await?;
  let mut cargo_inspected: HashMap<String, bool> = HashMap::new();
  for container_summary in containers {
    // extract cargo name and namespace
    let labels = container_summary.labels.unwrap_or_default();
    let Some(cargo_key) = labels.get("io.nanocl.cargo") else {
      // We don't have cargo label, we skip it
      continue;
    };
    let metadata = cargo_key.split('-').collect::<Vec<&str>>();
    if metadata.len() < 2 {
      // We don't have cargo label well formated, we skip it
      continue;
    }
    // If we already inspected this cargo we skip it
    if cargo_inspected.contains_key(metadata[1]) {
      continue;
    }

    // We inspect the container to have all the information we need
    let container = docker_api
      .inspect_container(
        &container_summary.id.unwrap_or_default(),
        None::<InspectContainerOptions>,
      )
      .await?;
    let config = container.config.unwrap_or_default();
    let mut config: bollard::container::Config<String> = config.into();
    config.host_config = container.host_config;
    let network_settings = container.network_settings.unwrap_or_default();
    if let Some(endpoints_config) = network_settings.networks {
      config.networking_config = Some(NetworkingConfig { endpoints_config });
    }

    let new_cargo = CargoConfigPartial {
      name: metadata[1..].join("-"),
      container: config.to_owned(),
      ..Default::default()
    };

    cargo_inspected.insert(metadata[1].to_owned(), true);
    match repositories::cargo::inspect_by_key(cargo_key.to_owned(), pool).await
    {
      // If the cargo is already in our store and the config is different we update it
      Ok(cargo) => {
        if cargo.config.container != config {
          log::debug!(
            "updating cargo {} in namespace {}",
            metadata[1],
            metadata[0]
          );
          repositories::cargo::update_by_key(
            cargo_key.to_owned(),
            new_cargo,
            pool,
          )
          .await?;
        }
        continue;
      }
      // unless we create his config
      Err(_err) => {
        log::debug!(
          "creating cargo {} in namespace {}",
          metadata[1],
          metadata[0]
        );
        repositories::cargo::create(metadata[0].to_owned(), new_cargo, pool)
          .await?;
      }
    }
  }

  Ok(())
}

/// Ensure exsistance of our deamon in the store.
/// We are running inside us it's that crazy ?
async fn register_daemon(arg: &ArgState) -> Result<(), DaemonError> {
  let key = utils::key::gen_key(&arg.sys_namespace, "daemon");
  if repositories::cargo::find_by_key(key, &arg.pool)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let path = Path::new(&arg.config.state_dir);
  let binds = vec![format!("{}:/var/lib/nanocl", path.display())];

  let container = bollard::container::Config::<String> {
    image: Some("nanocl-daemon:0.1.11".into()),
    domainname: Some("daemon".into()),
    host_config: Some(bollard::models::HostConfig {
      network_mode: Some("host".into()),
      restart_policy: Some(bollard::models::RestartPolicy {
        name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
        ..Default::default()
      }),
      binds: Some(binds),
      ..Default::default()
    }),
    ..Default::default()
  };

  let store_cargo = CargoConfigPartial {
    name: "daemon".into(),
    container,
    ..Default::default()
  };

  repositories::cargo::create(
    arg.sys_namespace.to_owned(),
    store_cargo,
    &arg.pool,
  )
  .await?;

  Ok(())
}

/// Register all dependencies needed
/// Default Namespace, Network, and Controllers will be registered in our store
async fn register_dependencies(arg: &ArgState) -> Result<(), DaemonError> {
  register_namespace(&arg.default_namespace, &arg.pool).await?;
  register_namespace(&arg.sys_namespace, &arg.pool).await?;
  utils::store::register(arg).await?;
  register_daemon(arg).await?;
  Ok(())
}

/// Init function called before http server start
/// to initialize our state
pub async fn init(config: &DaemonConfig) -> Result<DaemonState, DaemonError> {
  let docker_api = bollard::Docker::connect_with_unix(
    &config.docker_host,
    120,
    bollard::API_DEFAULT_VERSION,
  )?;
  ensure_system_network(&docker_api).await?;
  println!("gg");
  let pool = ensure_store(&config, &docker_api).await?;
  let arg_state = ArgState {
    pool: pool.to_owned(),
    config: config.to_owned(),
    default_namespace: String::from("global"),
    sys_namespace: String::from("system"),
  };
  register_dependencies(&arg_state).await?;
  sync_containers(&docker_api, &pool).await?;
  Ok(DaemonState {
    pool,
    docker_api,
    config: config.to_owned(),
  })
}

/// Init unit test
#[cfg(test)]
mod tests {
  use super::*;

  use crate::config;
  use crate::cli::Cli;
  use crate::utils::tests::*;

  /// Test init
  #[ntex::test]
  async fn basic_init() -> TestRet {
    // Init cli args
    let args = Cli {
      init: false,
      hosts: None,
      docker_host: None,
      state_dir: None,
      config_dir: String::from("/etc/nanocl"),
    };

    let config = config::init(&args)?;

    // test function init
    let _ = init(&config).await?;

    Ok(())
  }
}
