use std::{net::ToSocketAddrs, time::Duration};

use ntex::{rt, web, time};
use diesel::{
  PgConnection,
  r2d2::{Pool as R2D2Pool, ConnectionManager},
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::config::DaemonConfig;

use crate::models::{Pool, DBConn};

/// Create a pool connection to the store `cockroachdb`
pub async fn create_pool(store_addr: &str) -> IoResult<Pool> {
  let store_addr = store_addr.to_owned();
  // let store_addr = std::env::var("STORE_ADDR")
  //   .unwrap_or("store.nanocl.internal:26258".to_owned());
  // let options = format!("/defaultdb?sslmode=verify-full&sslcert={state_dir}/store/certs/client.root.crt&sslkey={state_dir}/store/certs/client.root.key&sslrootcert={state_dir}/store/certs/ca.crt");
  // let db_url = format!("postgresql://root:root@{host}{options}");
  let pool = web::block(move || {
    let manager = ConnectionManager::<PgConnection>::new(store_addr);
    R2D2Pool::builder().build(manager)
  })
  .await
  .map_err(|err| {
    IoError::interrupted("CockroachDB", &format!("Unable to create pool {err}"))
  })?;
  Ok(pool)
}

/// Get connection from the connection pool for the store `cockroachdb`
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

/// Wait for store to be ready to accept tcp connection.
/// We loop until a tcp connection can be established to the store.
async fn wait(store_addr: &str) -> IoResult<()> {
  let url = url::Url::parse(store_addr).map_err(|err| {
    IoError::invalid_data(
      "Wait store",
      &format!("invalid address format {err}"),
    )
  })?;
  let host_addr = url
    .host_str()
    .ok_or(IoError::invalid_data("Wait store", "Invalid store address"))?;
  let port = url
    .port()
    .ok_or(IoError::invalid_data("Wait store", "Invalid store port"))?;
  let store_addr = format!("{host_addr}:{port}");
  log::debug!("store::wait: {store_addr}");
  // Open tcp connection to check if store is ready
  let addr = store_addr
    .to_socket_addrs()
    .map_err(|err| {
      IoError::invalid_data(
        "Wait store",
        &format!("invalid address format {err}"),
      )
    })?
    .next()
    .expect("Unable to resolve store address");
  log::info!("store::wait: {addr}");
  while let Err(_err) = rt::tcp_connect(addr).await {
    log::warn!("store::wait: retry in 2s");
    time::sleep(Duration::from_secs(2)).await;
  }
  time::sleep(Duration::from_secs(2)).await;
  log::info!("store::wait: ready");
  Ok(())
}

/// Ensure existence of a container for our store.
/// We use cockroachdb with a postgresql connector.
/// We also run latest migration on our database to have the latest schema.
/// It will return a connection Pool that will be use in our State.
pub async fn init(daemon_conf: &DaemonConfig) -> IoResult<Pool> {
  const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
  let store_addr = match &daemon_conf.store_addr {
    None => {
      return Err(IoError::invalid_data(
        "Store",
        "Store address not specified",
      ))
    }
    Some(addr) => addr,
  };
  log::info!("store::init: {store_addr}");
  wait(store_addr).await?;
  let pool = create_pool(store_addr).await?;
  let mut conn = get_pool_conn(&pool)?;
  log::info!("store::init: migrations running");
  conn.run_pending_migrations(MIGRATIONS).map_err(|err| {
    IoError::interrupted("CockroachDB migration", &format!("{err}"))
  })?;
  log::info!("store::init: migrations success");
  Ok(pool)
}
