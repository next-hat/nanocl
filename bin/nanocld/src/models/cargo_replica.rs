use diesel::{
  backend::Backend,
  deserialize::{self, FromSql},
  prelude::*,
  serialize::{self, ToSql},
  sql_types::VarChar,
};

use crate::schema::{cargo_replica_processes, cargo_replicas};

/// Stable identity and current node assignment for one Cargo replica.
#[derive(Debug, Clone, PartialEq, Eq, Queryable, Identifiable, Selectable)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargo_replicas)]
pub(crate) struct CargoReplicaDb {
  pub(crate) key: uuid::Uuid,
  pub(crate) cargo_key: String,
  pub(crate) ordinal: i32,
  pub(crate) node_name: Option<String>,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) updated_at: chrono::NaiveDateTime,
}

/// Values used to create a stable Cargo replica identity.
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = cargo_replicas)]
pub(crate) struct NewCargoReplicaDb {
  pub(crate) key: uuid::Uuid,
  pub(crate) cargo_key: String,
  pub(crate) ordinal: i32,
  pub(crate) node_name: Option<String>,
}

impl NewCargoReplicaDb {
  pub(crate) fn new(cargo_key: impl Into<String>, ordinal: i32) -> Self {
    Self {
      key: uuid::Uuid::new_v4(),
      cargo_key: cargo_key.into(),
      ordinal,
      node_name: None,
    }
  }
}

/// Node-assignment update for one Cargo replica.
#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = cargo_replicas)]
#[diesel(treat_none_as_null = true)]
pub(crate) struct CargoReplicaUpdateDb {
  pub(crate) node_name: Option<String>,
  pub(crate) updated_at: chrono::NaiveDateTime,
}

impl CargoReplicaUpdateDb {
  pub(crate) fn node(node_name: Option<&str>) -> Self {
    Self {
      node_name: node_name.map(str::to_owned),
      updated_at: chrono::Utc::now().naive_utc(),
    }
  }
}

/// Logical role of one process inside a Cargo replica.
#[derive(
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  Hash,
  diesel::AsExpression,
  diesel::FromSqlRow,
)]
#[diesel(sql_type = VarChar)]
pub(crate) enum CargoReplicaProcessRole {
  Sandbox,
  Init,
  App,
}

impl CargoReplicaProcessRole {
  pub(crate) const fn as_str(self) -> &'static str {
    match self {
      Self::Sandbox => "sandbox",
      Self::Init => "init",
      Self::App => "app",
    }
  }
}

impl std::fmt::Display for CargoReplicaProcessRole {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(self.as_str())
  }
}

impl std::str::FromStr for CargoReplicaProcessRole {
  type Err = std::io::Error;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value {
      "sandbox" => Ok(Self::Sandbox),
      "init" => Ok(Self::Init),
      "app" => Ok(Self::App),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("invalid Cargo replica process role {value:?}"),
      )),
    }
  }
}

impl<DB> FromSql<VarChar, DB> for CargoReplicaProcessRole
where
  DB: Backend,
  String: FromSql<VarChar, DB>,
{
  fn from_sql(bytes: DB::RawValue<'_>) -> deserialize::Result<Self> {
    use std::str::FromStr;

    let value = String::from_sql(bytes)?;
    Self::from_str(&value).map_err(Into::into)
  }
}

impl<DB> ToSql<VarChar, DB> for CargoReplicaProcessRole
where
  DB: Backend,
  str: ToSql<VarChar, DB>,
{
  fn to_sql<'b>(
    &'b self,
    out: &mut serialize::Output<'b, '_, DB>,
  ) -> serialize::Result {
    self.as_str().to_sql(out)
  }
}

/// Stable logical process mapping for one named Cargo replica container.
#[derive(Debug, Clone, PartialEq, Eq, Queryable, Identifiable, Selectable)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargo_replica_processes)]
pub(crate) struct CargoReplicaProcessDb {
  pub(crate) key: uuid::Uuid,
  pub(crate) replica_key: uuid::Uuid,
  pub(crate) process_key: Option<String>,
  pub(crate) container_name: String,
  pub(crate) role: CargoReplicaProcessRole,
  pub(crate) essential: bool,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) updated_at: chrono::NaiveDateTime,
}

/// Values used to create one logical Cargo replica process mapping.
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = cargo_replica_processes)]
pub(crate) struct NewCargoReplicaProcessDb {
  pub(crate) key: uuid::Uuid,
  pub(crate) replica_key: uuid::Uuid,
  pub(crate) process_key: Option<String>,
  pub(crate) container_name: String,
  pub(crate) role: CargoReplicaProcessRole,
  pub(crate) essential: bool,
}

impl NewCargoReplicaProcessDb {
  pub(crate) fn new(
    replica_key: uuid::Uuid,
    container_name: impl Into<String>,
    role: CargoReplicaProcessRole,
    essential: bool,
  ) -> Self {
    Self {
      key: uuid::Uuid::new_v4(),
      replica_key,
      process_key: None,
      container_name: container_name.into(),
      role,
      essential,
    }
  }
}

/// Concrete-process attachment update for one logical Cargo process.
#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = cargo_replica_processes)]
#[diesel(treat_none_as_null = true)]
pub(crate) struct CargoReplicaProcessUpdateDb {
  pub(crate) process_key: Option<String>,
  pub(crate) updated_at: chrono::NaiveDateTime,
}

impl CargoReplicaProcessUpdateDb {
  pub(crate) fn process(process_key: Option<&str>) -> Self {
    Self {
      process_key: process_key.map(str::to_owned),
      updated_at: chrono::Utc::now().naive_utc(),
    }
  }
}
