use std::sync::Arc;

use diesel::PgConnection;
use diesel::r2d2::{Pool as R2D2Pool, PooledConnection, ConnectionManager};

mod ws;
pub use ws::*;

mod node;
pub use node::*;

mod system;
pub use system::*;

mod metric;
pub use metric::*;

mod namespace;
pub use namespace::*;

mod cargo;
pub use cargo::*;

pub mod vm;
pub use vm::*;

pub mod vm_image;
pub use vm_image::*;

mod resource;
pub use resource::*;

mod resource_kind;
pub use resource_kind::*;

mod secret;
pub use secret::*;

mod job;
pub use job::*;

mod spec;
pub use spec::*;

mod process;
pub use process::*;

mod event;
pub use event::*;

mod raw_emitter;
pub use raw_emitter::*;

<<<<<<< Updated upstream
mod task_manager;
pub use task_manager::*;

mod object_process_status;
pub use object_process_status::*;
=======
mod process_status;
pub use process_status::*;
>>>>>>> Stashed changes

pub type Pool = Arc<R2D2Pool<ConnectionManager<PgConnection>>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;

/// Generate a where clause for a string column
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
      _ => {
        // Ignore unsupported clause
      }
    }
  };
}

/// Generate a where clause for a json column
#[macro_export]
macro_rules! gen_where4json {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::IsNull => {
        $query = $query.filter($column.is_null());
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        $query = $query.filter($column.is_not_null());
      }
      nanocl_stubs::generic::GenericClause::Contains(val) => {
        $query = $query.filter($column.contains(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::HasKey(val) => {
        $query = $query.filter($column.has_key(val.clone()));
      }
      _ => {
        // Ignore unsupported clause
      }
    }
  };
}

#[macro_export]
macro_rules! gen_where4uuid {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::IsNull => {
        $query = $query.filter($column.is_null());
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        $query = $query.filter($column.is_not_null());
      }
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        let uuid = uuid::Uuid::parse_str(&val).unwrap_or_default();
        $query = $query.filter($column.eq(uuid));
      }
      _ => {
        // Ignore unsupported clause
      }
    }
  };
}

#[macro_export]
macro_rules! gen_multiple {
  ($query: expr, $column: expr, $filter: expr) => {
    $query = $query.order($column.desc());
    let limit = $filter.limit.unwrap_or(100);
    $query = $query.limit(limit as i64);
    if let Some(offset) = $filter.offset {
      $query = $query.offset(offset as i64);
    }
  };
}
