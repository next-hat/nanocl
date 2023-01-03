use diesel::PgConnection;
use r2d2::PooledConnection;
use diesel::r2d2::ConnectionManager;

mod state;
pub use state::*;

mod namespace;
pub use namespace::*;

mod cargo;
pub use cargo::*;

mod cargo_config;
pub use cargo_config::*;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;
