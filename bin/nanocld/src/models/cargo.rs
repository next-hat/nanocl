use diesel::prelude::*;
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPatch};

use crate::schema::cargoes;

use super::NamespaceDb;

/// This structure represent the cargo in the database.
/// A cargo is a replicable container that can be used to deploy a service.
/// His specification is stored as a relation to a `CargoSpecDb`.
/// To keep track of the history of the cargo.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargoes)]
#[diesel(belongs_to(NamespaceDb, foreign_key = namespace_name))]
pub struct CargoDb {
  /// The key of the cargo generated with `namespace_name` and `name`
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The name of the cargo
  pub name: String,
  /// The spec key reference
  pub spec_key: uuid::Uuid,
  /// The status key reference
  pub status_key: String,
  /// The namespace name
  pub namespace_name: String,
  /// Immutable identity retained after this live definition is deleted.
  pub incarnation_key: uuid::Uuid,
}

#[derive(Debug)]
pub(crate) struct AcceptedCargoOperation {
  pub cargo: nanocl_stubs::cargo::Cargo,
  pub operation: super::OperationDb,
}

/// Versioned node observation carried in the existing HTTP error message.
/// It is never authoritative for operation settlement.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CargoNodeMissingSecretReport {
  pub kind: String,
  pub operation: uuid::Uuid,
  pub incarnation: uuid::Uuid,
  pub spec: uuid::Uuid,
  pub name: String,
}

pub(crate) type CargoLifecycleLock = std::sync::Arc<tokio::sync::Mutex<()>>;
pub(crate) type CargoLifecycleLocks = std::sync::Mutex<
  std::collections::HashMap<String, std::sync::Weak<tokio::sync::Mutex<()>>>,
>;
pub(crate) type CargoOperationLease = (
  std::sync::Arc<tokio::sync::OwnedMutexGuard<()>>,
  super::OperationGuard,
);
pub(crate) type CargoRuntimeLease = (
  std::sync::Arc<tokio::sync::OwnedMutexGuard<()>>,
  Option<super::OperationGuard>,
);

/// Captured operation identity and worker lifetime, including delegated workers.
#[derive(Clone)]
pub(crate) struct CargoRuntimeContext {
  pub operation: super::OperationDb,
  pub lease: CargoRuntimeLease,
  pub state: super::SystemState,
}

impl std::ops::Deref for CargoRuntimeContext {
  type Target = super::SystemState;

  fn deref(&self) -> &Self::Target {
    &self.state
  }
}

/// This structure is used to update a cargo in the database.
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = cargoes)]
pub struct CargoUpdateDb {
  /// The key of the cargo generated with `namespace_name` and `name`
  pub key: Option<String>,
  /// The name of the cargo
  pub name: Option<String>,
  /// The namespace name
  pub namespace_name: Option<String>,
  /// The spec key reference
  pub spec_key: Option<uuid::Uuid>,
}

/// Arguments to create a new cargo obj
pub struct CargoObjCreateIn {
  pub namespace: String,
  pub spec: CargoSpec,
  pub version: String,
}

/// Arguments to create a new cargo history entry
pub struct CargoObjPutIn {
  pub spec: CargoSpec,
  pub version: String,
}

/// Arguments to update a cargo history entry
pub struct CargoObjPatchIn {
  pub spec: CargoSpecPatch,
  pub version: String,
}
