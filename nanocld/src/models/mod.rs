use diesel::PgConnection;
use r2d2::PooledConnection;
use diesel::r2d2::ConnectionManager;

mod state;
pub use state::*;

mod namespace;
pub use namespace::*;

mod cargo;
pub use cargo::*;

mod resource;
pub use resource::*;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;
