use std::sync::Arc;
use std::time::Duration;
use std::net::ToSocketAddrs;

use ntex::{rt, web, time};
use diesel::PgConnection;
use diesel::r2d2::{Pool as R2D2Pool, ConnectionManager};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use nanocl_stubs::config::DaemonConfig;

use nanocl_error::io::{IoError, IoResult};

use crate::models::{Pool, DBConn};

/// Create a pool connection to the store `cockroachdb`
pub(crate) async fn create_pool(
  host: &str,
  daemon_conf: &DaemonConfig,
) -> IoResult<Pool> {
  let state_dir = daemon_conf.state_dir.clone();
  let options = format!("/defaultdb?sslmode=verify-full&sslcert={state_dir}/store/certs/client.root.crt&sslkey={state_dir}/store/certs/client.root.key&sslrootcert={state_dir}/store/certs/ca.crt");
  let db_url = format!("postgresql://root:root@{host}{options}");
  let pool = web::block(move || {
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    R2D2Pool::builder().build(manager)
  })
  .await
  .map_err(|err| {
    IoError::interupted("CockroachDB", &format!("Unable to create pool {err}"))
  })?;
  Ok(Arc::new(pool))
}

/// Get connection from the connection pool for the store `cockroachdb`
pub(crate) fn get_pool_conn(pool: &Pool) -> IoResult<DBConn> {
  let conn = match pool.get() {
    Ok(conn) => conn,
    Err(err) => {
      return Err(IoError::new(
        "CockroachDB connection",
        std::io::Error::new(std::io::ErrorKind::NotConnected, err),
      ))
    }
  };
  Ok(conn)
}

/// Wait for store to be ready to accept tcp connection.
/// We loop until a tcp connection can be established to the store.
async fn wait_store(addr: &str) -> IoResult<()> {
  // Open tcp connection to check if store is ready
  let addr = addr
    .to_socket_addrs()
    .map_err(|err| {
      IoError::invalid_data(
        "Wait store",
        &format!("invalid address format {err}"),
      )
    })?
    .next()
    .expect("Unable to resolve store address");
  while let Err(_err) = rt::tcp_connect(addr).await {
    log::warn!("Waiting for store");
    time::sleep(Duration::from_secs(2)).await;
  }
  time::sleep(Duration::from_secs(2)).await;
  Ok(())
}

/// Ensure existance of a container for our store.
/// We use cockroachdb with a postgresql connector.
/// We also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
pub(crate) async fn init(daemon_conf: &DaemonConfig) -> IoResult<Pool> {
  const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
  let store_addr = std::env::var("STORE_URL")
    .unwrap_or("nstore.nanocl.internal:26258".to_owned());
  log::info!("Connecting to store at: {store_addr}");
  wait_store(&store_addr).await?;
  let pool = create_pool(&store_addr, daemon_conf).await?;
  let mut conn = get_pool_conn(&pool)?;
  log::info!("Store connected, running migrations");
  conn.run_pending_migrations(MIGRATIONS).map_err(|err| {
    IoError::interupted("CockroachDB migration", &format!("{err}"))
  })?;
  log::info!("Migrations successfully applied");
  Ok(pool)
}
