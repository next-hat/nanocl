use ntex::web;
use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::models::{Pool, DBConn};

/// ## Create pool
///
/// Create a pool connection to cockroachdb
///
/// ## Arguments
///
/// [host](String) Host to connect to
///
/// ## Returns
///
/// - [Pool](Pool) R2d2 pool connection for postgres
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils;
///
/// let pool = utils::create_pool("localhost".to_string()).await;
/// ```
///
pub async fn create_pool(host: String) -> IoResult<Pool> {
  // ?sslmode=verify-full
  web::block(move || {
    let db_url =
      "postgres://root:root@".to_owned() + &host + ":26257/defaultdb";
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    r2d2::Pool::builder().build(manager)
  })
  .await
  .map_err(|err| {
    IoError::interupted("CockroachDB", &format!("Unable to create pool {err}"))
  })
}

/// ## Get store ip address
///
/// Get the ip address of the store container
///
/// ## Arguments
///
/// [docker_api](Docker) Reference to docker api
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](String) - The ip address of the store
///   - [Err](HttpResponseError) - The ip address of the store has not been retrieved
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let ip_address = utils::store::get_store_ip_addr(&docker_api).await;
/// ```
///
pub async fn get_store_addr(
  docker_api: &bollard_next::Docker,
) -> IoResult<String> {
  let container = docker_api
    .inspect_container("nstore.system.c", None)
    .await
    .map_err(|err| {
      err.map_err_context(|| "Unable to inspect nstore.system.c container")
    })?;
  let networks = container
    .network_settings
    .unwrap_or_default()
    .networks
    .unwrap_or_default();
  let ip_address = networks
    .get("system")
    .ok_or(IoError::invalid_data("Network", "system not found"))?
    .ip_address
    .as_ref()
    .ok_or(IoError::invalid_data("IpAddress", "not detected"))?;
  Ok(ip_address.to_owned())
}

/// ## Get connection from the pool
///
/// Get connection from the connection pool
///
/// ## Arguments
///
/// [pool](Pool) a pool wrapped in ntex State
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](DBConn) - The connection has been retrieved
///   - [Err](HttpResponseError) - The connection has not been retrieved
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils;
///
/// let pool = utils::store::create_pool("localhost".to_string()).await;
/// let conn = utils::store::get_pool_conn(&pool);
/// ```
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
