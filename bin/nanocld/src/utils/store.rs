use ntex::web;
use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;

use nanocl_utils::io_error::{IoError, IoResult};

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
pub async fn create_pool(host: &str) -> IoResult<Pool> {
  // ?sslmode=verify-full
  let db_url = "postgres://root:root@".to_owned() + host + "/defaultdb";
  web::block(move || {
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    r2d2::Pool::builder().build(manager)
  })
  .await
  .map_err(|err| {
    IoError::interupted("CockroachDB", &format!("Unable to create pool {err}"))
  })
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
