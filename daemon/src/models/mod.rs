use r2d2::PooledConnection;
use serde::{Deserialize, Serialize};
use diesel::{r2d2::ConnectionManager, PgConnection};

#[cfg(feature = "dev")]
use utoipa::ToSchema;

mod config;
pub use config::*;

mod state;
pub use state::*;

mod namespace;
pub use namespace::*;

mod cargo;
pub use cargo::*;

mod cargo_config;
pub use cargo_config::*;

mod cargo_image;
pub use cargo_image::*;

mod system;
pub use system::*;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;

/// Generic delete response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "dev", derive(ToSchema))]
pub struct GenericDelete {
  pub(crate) count: usize,
}

/// Generic count response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "dev", derive(ToSchema))]
pub struct GenericCount {
  pub(crate) count: i64,
}

/// Generic namespace query filter
#[derive(Debug, Serialize, Deserialize)]
pub struct GenericNspQuery {
  pub(crate) namespace: Option<String>,
}
