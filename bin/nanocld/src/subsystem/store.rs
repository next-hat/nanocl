use std::net::SocketAddr;
use std::time::Duration;

use ntex::rt;
use ntex::time;

use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use crate::utils;
use crate::models::Pool;
use crate::error::CliError;

/// Wait for store to be ready
/// We loop until a tcp connection can be established to the store
async fn wait_store(addr: &str) -> Result<(), CliError> {
  // Open tcp connection to check if store is ready
  let addr: SocketAddr = addr.parse().map_err(|err| {
    CliError::new(1, format!("Failed to parse store address: {}", err))
  })?;
  while let Err(_err) = rt::tcp_connect(addr).await {
    log::warn!("Waiting for store");
    time::sleep(Duration::from_millis(1000)).await;
  }
  Ok(())
}

/// Ensure existance of a container for our store
/// we use cockroachdb with a postgresql connector.
/// we also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
pub(crate) async fn init(
  docker_api: &bollard_next::Docker,
) -> Result<Pool, CliError> {
  const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
  let store_addr = utils::store::get_store_addr(docker_api).await?;
  log::info!("Connecting to store at: {store_addr}:26257");
  wait_store(&format!("{store_addr}:26257")).await?;
  // Connect to postgresql
  let pool = utils::store::create_pool(store_addr).await?;
  let mut conn = utils::store::get_pool_conn(&pool)?;
  log::info!("Store connected");
  // This will run the necessary migrations.
  // See the documentation for `MigrationHarness` for
  // all available methods.
  log::info!("Running migrations");
  conn.run_pending_migrations(MIGRATIONS).map_err(|err| {
    CliError::new(1, format!("Failed to run sql migrations: {}", err))
  })?;
  log::info!("Migrations done");
  Ok(pool)
}
