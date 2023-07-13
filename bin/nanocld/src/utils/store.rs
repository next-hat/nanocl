use std::time::Duration;
use std::net::ToSocketAddrs;

use ntex::rt;
use ntex::web;
use ntex::time;
use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use nanocl_utils::io_error::{IoError, IoResult};

use crate::utils;
use crate::models::{Pool, DBConn};

/// ## Create pool
///
/// Create a pool connection to the store `cockroachdb`
///
/// ## Arguments
///
/// - [host](str) Host to connect to
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](Pool) - The pool has been created
///   - [Err](IoError) - The pool has not been created
///
pub async fn create_pool(host: &str) -> IoResult<Pool> {
  // let options = format!("/defaultdb?sslmode=verify-full&sslrootcert=/var/lib/nanocl/store/certs/ca.crt");
  let options = "/defaultdb";
  let db_url = format!("postgres://root:root@{host}{options}");
  web::block(move || {
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    r2d2::Pool::builder().build(manager)
  })
  .await
  .map_err(|err| {
    IoError::interupted("CockroachDB", &format!("Unable to create pool {err}"))
  })
}

/// ## Get pool conn
///
/// Get connection from the connection pool for the store `cockroachdb`
///
/// ## Arguments
///
/// [pool](Pool) a pool wrapped in ntex State
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](DBConn) - The connection has been retrieved
///   - [Err](IoError) - The connection has not been retrieved
///
pub fn get_pool_conn(pool: &Pool) -> IoResult<DBConn> {
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

/// ## Wait store
///
/// Wait for store to be ready to accept tcp connection.
/// We loop until a tcp connection can be established to the store.
///
/// ## Arguments
///
/// - [addr](str) Address of the store
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](()) - The store is ready
///   - [Err](IoError) - The store is not ready
///
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
    .unwrap();
  while let Err(_err) = rt::tcp_connect(addr).await {
    log::warn!("Waiting for store");
    time::sleep(Duration::from_secs(2)).await;
  }
  time::sleep(Duration::from_secs(4)).await;
  Ok(())
}

/// ## Init
///
/// Ensure existance of a container for our store.
/// We use cockroachdb with a postgresql connector.
/// We also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](Pool) - The pool has been created
///   - [Err](IoError) - The pool has not been created
///
pub(crate) async fn init() -> IoResult<Pool> {
  const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
  let store_addr = "nstore.nanocl.internal:26257";
  // let store_addr = utils::store::get_store_addr(docker_api).await?;
  log::info!("Connecting to store at: {store_addr}");
  wait_store(store_addr).await?;
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
