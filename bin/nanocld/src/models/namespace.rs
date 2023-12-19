use std::sync::Arc;

use diesel::prelude::*;
use ntex::rt::JoinHandle;
use serde::{Serialize, Deserialize};

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{generic::GenericFilter, namespace::NamespacePartial};

use crate::{utils, schema::namespaces};

use super::{Pool, Repository};

/// This structure represent the namespace in the database.
/// A namespace is a group of cargo or virtual machine that share the same network.
/// It is used to isolate the services.
#[derive(
  Debug, Clone, Serialize, Deserialize, Identifiable, Insertable, Queryable,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = namespaces)]
#[serde(rename_all = "PascalCase")]
pub struct NamespaceDb {
  /// The name of the namespace
  pub name: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
}

impl NamespaceDb {
  /// Create a new namespace
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
    }
  }
}

impl From<&NamespacePartial> for NamespaceDb {
  fn from(p: &NamespacePartial) -> Self {
    Self {
      name: p.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
    }
  }
}

impl Repository for NamespaceDb {
  type Table = namespaces::table;
  type Item = NamespaceDb;
  type UpdateItem = NamespaceDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::trace!("NamespaceDb::find_one: {filter:?}");
    // let r#where = filter.r#where.to_owned().unwrap_or_default();
    let query = namespaces::dsl::namespaces
      .order(namespaces::dsl::created_at.desc())
      .into_boxed();
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::trace!("NamespaceDb::find: {filter:?}");
    // let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = namespaces::dsl::namespaces
      .order(namespaces::dsl::created_at.desc())
      .into_boxed();
    let limit = filter.limit.unwrap_or(100);
    query = query.limit(limit as i64);
    if let Some(offset) = filter.offset {
      query = query.offset(offset as i64);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(items)
    })
  }
}
