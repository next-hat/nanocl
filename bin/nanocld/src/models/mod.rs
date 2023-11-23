use diesel::PgConnection;
use r2d2::PooledConnection;
use diesel::r2d2::ConnectionManager;

mod ws;
pub use ws::*;

mod node;
pub use node::*;

mod state;
pub use state::*;

mod metric;
pub use metric::*;

mod http_metric;
pub use http_metric::*;

mod namespace;
pub use namespace::*;

mod cargo;
pub use cargo::*;

mod cargo_spec;
pub use cargo_spec::*;

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

mod resource_spec;
pub use resource_spec::*;

mod secret;
pub use secret::*;

mod job;
pub use job::*;

mod container_instance;
pub use container_instance::*;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;
