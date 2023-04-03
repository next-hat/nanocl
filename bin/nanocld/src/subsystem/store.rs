use std::time;
use std::thread;

use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use nanocl_stubs::config::DaemonConfig;

use crate::utils;
use crate::models::Pool;
use crate::error::CliError;

/// Ensure existance of a container for our store
/// we use cockroachdb with a postgresql connector.
/// we also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
pub(crate) async fn init(
  docker_api: &bollard_next::Docker,
  daemon_conf: &DaemonConfig,
) -> Result<Pool, CliError> {
  let mut store_conf = include_str!("../../specs/store.yml").to_owned();
  store_conf = store_conf.replace("{state_dir}", &daemon_conf.state_dir);
  store_conf =
    store_conf.replace("{advertise_addr}", &daemon_conf.advertise_addr);
  const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
  log::info!("Booting store");
  super::system::start_subsystem(docker_api, &store_conf, daemon_conf).await?;
  let postgresql_id = utils::store::get_store_ip_addr(docker_api).await?;
  // We wait 1000ms to ensure store is booted
  // It's a tricky hack to avoid some error printed by postgresql connector for now.
  thread::sleep(time::Duration::from_millis(1000));
  log::info!("Connecting to store");
  // Connect to postgresql
  let pool = utils::store::create_pool(postgresql_id).await;
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
