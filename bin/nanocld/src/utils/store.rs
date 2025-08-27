use std::{net::ToSocketAddrs, time::Duration};

use diesel::{
  r2d2::{ConnectionManager, Pool as R2D2Pool},
  PgConnection,
};
use diesel_migrations::{
  embed_migrations, EmbeddedMigrations, MigrationHarness,
};
use ntex::{rt, time, web};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  config::DaemonConfig, proxy::ProxySslConfig, secret::SecretPartial,
};

use crate::{
  models::{DBConn, Pool, SecretDb},
  repositories::generic::*,
};

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
      return Err(IoError::with_context(
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

async fn save_db_cert(store_addr: &str, pool: &Pool) -> IoResult<()> {
  if SecretDb::read_by_pk("cert.db.nanocl.io", pool)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let url = url::Url::parse(store_addr).map_err(|err| {
    IoError::invalid_data(
      "Save DB cert",
      &format!("invalid address format {err}"),
    )
  })?;
  // extract sslcert sslkey sslrootcert from query params
  let mut query_pairs = url.query_pairs();
  let sslcert = query_pairs.find(|(k, _)| k == "sslcert").map(|(_, v)| v);
  let sslkey = query_pairs.find(|(k, _)| k == "sslkey").map(|(_, v)| v);
  let sslrootcert = query_pairs
    .find(|(k, _)| k == "sslrootcert")
    .map(|(_, v)| v);
  match (sslcert, sslkey, sslrootcert) {
    (Some(sslcert), Some(sslkey), Some(sslrootcert)) => {
      SecretDb::create_from(
        &SecretPartial {
          name: "cert.db.nanocl.io".to_owned(),
          kind: "nanocl.io/tls".to_owned(),
          immutable: false,
          metadata: None,
          data: serde_json::to_value(ProxySslConfig {
            certificate: sslcert.to_string(),
            certificate_key: sslkey.to_string(),
            certificate_client: Some(sslrootcert.to_string()),
            verify_client: None,
            dhparam: None,
          })?,
        },
        pool,
      )
      .await?;
    }
    _ => {
      log::warn!("store::save_db_cert: missing certs");
    }
  }
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
  save_db_cert(store_addr, &pool).await?;
  log::info!("store::init: migrations success");
  Ok(pool)
}
