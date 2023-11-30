use nanocl_error::io::IoResult;
use nanocl_stubs::{generic::GenericFilter, namespace::NamespacePartial};
use serde::{Serialize, Deserialize};
use tokio::task::JoinHandle;

use crate::schema::namespaces;

use super::{Repository, Pool};

/// ## NamespaceDb
///
/// This structure represent the namespace in the database.
/// A namespace is a group of cargo or virtual machine that share the same network.
/// It is used to isolate the services.
///
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

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
  }
}
