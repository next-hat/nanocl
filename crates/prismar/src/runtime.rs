use diesel::{
  Connection,
  r2d2::{ConnectionManager, Pool},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
  #[error("failed to acquire diesel connection from pool: {0}")]
  Pool(String),
  #[error("blocking query task interrupted: {0}")]
  Join(String),
  #[error("diesel query failed: {0}")]
  Query(String),
}

pub async fn with_connection<Conn, R, F>(
  pool: Pool<ConnectionManager<Conn>>,
  operation: F,
) -> Result<R, RuntimeError>
where
  Conn: Connection + diesel::r2d2::R2D2Connection + 'static,
  R: Send + 'static,
  F: FnOnce(&mut Conn) -> diesel::QueryResult<R> + Send + 'static,
{
  ntex::rt::spawn_blocking(move || {
    let mut connection = pool
      .get()
      .map_err(|err| RuntimeError::Pool(err.to_string()))?;
    operation(&mut connection)
      .map_err(|err| RuntimeError::Query(err.to_string()))
  })
  .await
  .map_err(|err| RuntimeError::Join(err.to_string()))?
}

pub async fn with_raw_query<Conn, R, F>(
  pool: Pool<ConnectionManager<Conn>>,
  query: crate::RawSqlQuery,
  operation: F,
) -> Result<R, RuntimeError>
where
  Conn: Connection + diesel::r2d2::R2D2Connection + 'static,
  R: Send + 'static,
  F: FnOnce(&mut Conn, crate::RawSqlQuery) -> diesel::QueryResult<R>
    + Send
    + 'static,
{
  with_connection(pool, move |conn| operation(conn, query)).await
}
