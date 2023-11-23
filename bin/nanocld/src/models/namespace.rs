use serde::{Serialize, Deserialize};

use crate::schema::namespaces;

/// ## NamespaceModel
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
  pub(crate) name: String,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
}
