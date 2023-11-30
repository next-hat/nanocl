use std::sync::Arc;

use diesel::PgConnection;
use diesel::r2d2::{Pool as R2D2Pool, PooledConnection, ConnectionManager};

mod ws;
pub use ws::*;

mod node;
pub use node::*;

mod generic;
pub use generic::*;

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

mod vm_spec;
pub use vm_spec::*;

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

mod container;
pub use container::*;

pub type Pool = Arc<R2D2Pool<ConnectionManager<PgConnection>>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;

#[macro_export]
macro_rules! gen_where4string {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        $query = $query.filter($column.eq(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Ne(val) => {
        $query = $query.filter($column.ne(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Gt(val) => {
        $query = $query.filter($column.gt(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Lt(val) => {
        $query = $query.filter($column.lt(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Ge(val) => {
        $query = $query.filter($column.ge(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Le(val) => {
        $query = $query.filter($column.le(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Like(val) => {
        $query = $query.filter($column.like(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::NotLike(val) => {
        $query = $query.filter($column.not_like(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::In(items) => {
        $query = $query.filter($column.eq_any(items.clone()));
      }
      nanocl_stubs::generic::GenericClause::NotIn(items) => {
        $query = $query.filter($column.ne_all(items.clone()));
      }
      nanocl_stubs::generic::GenericClause::IsNull => {
        $query = $query.filter($column.is_null());
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        $query = $query.filter($column.is_not_null());
      }
    }
  };
}
