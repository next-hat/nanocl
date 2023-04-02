use ntex::web;
use ntex::http::StatusCode;
use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;

use crate::error::HttpResponseError;
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
pub async fn create_pool(host: String) -> Pool {
  // ?sslmode=verify-full
  web::block(move || {
    let db_url = "postgres://root:root@".to_owned()
      + &host
      + ":26257/defaultdb?sslmode=require";
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    r2d2::Pool::builder().build(manager)
  })
  .await
  .expect("Cannot connect to the store")
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
pub fn get_pool_conn(pool: &Pool) -> Result<DBConn, HttpResponseError> {
  let conn = match pool.get() {
    Ok(conn) => conn,
    Err(err) => {
      return Err(HttpResponseError {
        msg: format!("Cannot get connection from pool got error: {}", &err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      });
    }
  };
  Ok(conn)
}
