use diesel::PgConnection;
use r2d2::PooledConnection;
use diesel::r2d2::ConnectionManager;

mod boot;
pub use boot::*;

mod node;
pub use node::*;

mod metric;
pub use metric::*;

mod namespace;
pub use namespace::*;

mod cargo;
pub use cargo::*;

mod cargo_config;
pub use cargo_config::*;

pub mod vm;
pub use vm::*;

mod vm_config;
pub use vm_config::*;

pub mod vm_image;
pub use vm_image::*;

mod resource;
pub use resource::*;

mod resource_kind;
pub use resource_kind::*;

mod resource_config;
pub use resource_config::*;

mod state;
pub use state::*;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;
