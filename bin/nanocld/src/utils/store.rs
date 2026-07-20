use std::{fs, net::ToSocketAddrs, path::Path, time::Duration};

use diesel::{
  PgConnection,
  r2d2::{ConnectionManager, Pool as R2D2Pool},
};
use diesel_migrations::{
  EmbeddedMigrations, MigrationHarness, embed_migrations,
};
use ntex::{rt, time, web};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  config::DaemonConfig,
  proxy::ProxySslConfig,
  secret::{SecretPartial, SecretUpdate},
};

use crate::{
  models::{DBConn, Pool, SecretDb},
  repositories::generic::*,
};

const DB_CERT_SECRET_NAME: &str = "cert.db.nanocl.io";

/// Read a legacy certificate path once, otherwise retain its PEM value.
///
/// The store URL still needs paths while creating the initial database
/// connection. The persisted TLS secret must not retain those paths.
fn load_certificate_value(
  value: &str,
  field: &str,
) -> IoResult<(String, bool)> {
  let path = Path::new(value);
  if !path.is_file() {
    return Ok((value.to_owned(), false));
  }
  let value = fs::read_to_string(path).map_err(|err| {
    IoError::interrupted(
      "Database certificate",
      &format!("Unable to read {field} from {}: {err}", path.display()),
    )
  })?;
  Ok((value, true))
}

fn load_db_tls_config(
  config: ProxySslConfig,
) -> IoResult<(ProxySslConfig, bool)> {
  let (certificate, certificate_changed) =
    load_certificate_value(&config.certificate, "certificate")?;
  let (certificate_key, certificate_key_changed) =
    load_certificate_value(&config.certificate_key, "private key")?;
  let (certificate_client, certificate_client_changed) =
    match config.certificate_client {
      Some(certificate_client) => {
        let (value, changed) =
          load_certificate_value(&certificate_client, "certificate authority")?;
        (Some(value), changed)
      }
      None => (None, false),
    };
  let (dhparam, dhparam_changed) = match config.dhparam {
    Some(dhparam) => {
      let (value, changed) = load_certificate_value(&dhparam, "DH parameters")?;
      (Some(value), changed)
    }
    None => (None, false),
  };
  Ok((
    ProxySslConfig {
      certificate,
      certificate_key,
      certificate_client,
      verify_client: config.verify_client,
      dhparam,
    },
    certificate_changed
      || certificate_key_changed
      || certificate_client_changed
      || dhparam_changed,
  ))
}

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
      ));
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
  while let Err(_err) = rt::tcp_connect(addr, ntex::SharedCfg::default()).await
  {
    log::warn!("store::wait: retry in 2s");
    time::sleep(Duration::from_secs(2)).await;
  }
  time::sleep(Duration::from_secs(2)).await;
  log::info!("store::wait: ready");
  Ok(())
}

async fn save_db_cert(store_addr: &str, pool: &Pool) -> IoResult<()> {
  if let Ok(existing) = SecretDb::read_by_pk(DB_CERT_SECRET_NAME, pool).await {
    let config = serde_json::from_value::<ProxySslConfig>(existing.data)?;
    let (config, changed) = load_db_tls_config(config)?;
    if changed {
      SecretDb::update_pk(
        DB_CERT_SECRET_NAME,
        &SecretUpdate {
          data: serde_json::to_value(config)?,
          metadata: None,
        },
        pool,
      )
      .await?;
    }
    return Ok(());
  }
  let url = url::Url::parse(store_addr).map_err(|err| {
    IoError::invalid_data(
      "Save DB cert",
      &format!("invalid address format {err}"),
    )
  })?;
  // Extract sslcert, sslkey and sslrootcert from query parameters. These
  // paths are only used to establish this first connection; their PEM values
  // are what is persisted in CockroachDB.
  let mut query_pairs = url.query_pairs();
  let sslcert = query_pairs.find(|(k, _)| k == "sslcert").map(|(_, v)| v);
  let sslkey = query_pairs.find(|(k, _)| k == "sslkey").map(|(_, v)| v);
  let sslrootcert = query_pairs
    .find(|(k, _)| k == "sslrootcert")
    .map(|(_, v)| v);
  match (sslcert, sslkey, sslrootcert) {
    (Some(sslcert), Some(sslkey), Some(sslrootcert)) => {
      let (certificate, _) = load_certificate_value(&sslcert, "certificate")?;
      let (certificate_key, _) =
        load_certificate_value(&sslkey, "private key")?;
      let (certificate_client, _) =
        load_certificate_value(&sslrootcert, "certificate authority")?;
      SecretDb::create_from(
        &SecretPartial {
          name: DB_CERT_SECRET_NAME.to_owned(),
          kind: "nanocl.io/tls".to_owned(),
          immutable: false,
          metadata: None,
          data: serde_json::to_value(ProxySslConfig {
            certificate,
            certificate_key,
            certificate_client: Some(certificate_client),
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
      ));
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
