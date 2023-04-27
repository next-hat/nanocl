use std::net::SocketAddr;
use std::time::Duration;

use ntex::rt;
use ntex::time;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use nanocl_utils::io_error::{IoError, IoResult};

use crate::utils;
use crate::models::Pool;

/// Wait for store to be ready
/// We loop until a tcp connection can be established to the store
async fn wait_store(addr: &str) -> IoResult<()> {
  // Open tcp connection to check if store is ready
  let addr: SocketAddr = addr.parse().map_err(|err| {
    IoError::invalid_data(
      "Wait store",
      &format!("invalid address format {err}"),
    )
  })?;
  while let Err(_err) = rt::tcp_connect(addr).await {
    log::warn!("Waiting for store");
    time::sleep(Duration::from_secs(2)).await;
  }
  time::sleep(Duration::from_secs(4)).await;
  Ok(())
}

/// Ensure existance of a container for our store
/// we use cockroachdb with a postgresql connector.
/// we also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
pub(crate) async fn init(docker_api: &bollard_next::Docker) -> IoResult<Pool> {
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
    IoError::interupted("CockroachDB migration", &format!("{err}"))
  })?;
  log::info!("Migrations done");
  Ok(pool)
}
