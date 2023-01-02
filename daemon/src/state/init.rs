//! File used to describe daemon boot
use std::path::Path;
use std::{time, thread};
use std::collections::HashMap;

use bollard::Docker;
use bollard::network::{CreateNetworkOptions, InspectNetworkOptions};

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use nanocl_models::cargo::CargoPartial;
use nanocl_models::cargo_config::CargoConfigPartial;

use crate::cli::Cli;
use crate::{utils, repositories};
use crate::models::{Pool, NamespacePartial, DaemonConfig, ArgState, DaemonState};

use crate::errors::DaemonError;

use super::config;

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
  match repositories::namespace::inspect_by_name(name.to_owned(), pool).await {
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
  println!("state dir {}", &arg.config.state_dir);
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

  let config = CargoConfigPartial {
    name: "daemon".into(),
    container,
    ..Default::default()
  };

  let store_cargo = CargoPartial {
    name: config.name.to_owned(),
    config,
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
pub async fn init(args: &Cli) -> Result<DaemonState, DaemonError> {
  let config = config::init(args)?;
  let docker_api = bollard::Docker::connect_with_unix(
    &config.docker_host,
    120,
    bollard::API_DEFAULT_VERSION,
  )?;
  ensure_system_network(&docker_api).await?;
  let pool = ensure_store(&config, &docker_api).await?;
  let arg_state = ArgState {
    pool: pool.to_owned(),
    config: config.to_owned(),
    default_namespace: String::from("global"),
    sys_namespace: String::from("system"),
  };
  register_dependencies(&arg_state).await?;
  Ok(DaemonState {
    pool,
    config,
    docker_api,
  })
}

/// Init unit test
#[cfg(test)]
mod tests {
  use super::*;

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

    // test function init
    let _ = init(&args).await?;

    Ok(())
  }
}
