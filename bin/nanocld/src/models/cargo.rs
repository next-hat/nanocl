use std::sync::Arc;
use std::collections::HashMap;

use ntex::web;
use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocl_stubs::cargo::Cargo;
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};
use nanocl_stubs::generic::{GenericFilter, GenericClause};

use crate::schema::cargoes;
use crate::{utils, gen_where4string};

use super::{Pool, CargoSpecDb};
use super::generic::{Repository, WithSpec, FromSpec};
use super::namespace::NamespaceDb;

/// This structure represent the cargo in the database.
/// A cargo is a replicable container that can be used to deploy a service.
/// His specification is stored as a relation to a `CargoSpecDb`.
/// To keep track of the history of the cargo.
#[derive(Debug, Queryable, Identifiable, Insertable, Associations)]
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
  /// The namespace name
  pub namespace_name: String,
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

impl WithSpec for CargoDb {
  type Type = Cargo;
  type Relation = CargoSpec;

  fn with_spec(self, r: &Self::Relation) -> Self::Type {
    Self::Type {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.clone(),
    }
  }
}

impl Repository for CargoDb {
  type Table = cargoes::table;
  type Item = Cargo;
  type UpdateItem = CargoUpdateDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    let mut query = cargoes::dsl::cargoes
      .inner_join(crate::schema::cargo_specs::table)
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    log::debug!("CargoDb::find_one filter: {:?}", r#where);
    if let Some(value) = r#where.get("Key") {
      gen_where4string!(query, cargoes::dsl::key, value);
    }
    if let Some(value) = r#where.get("Name") {
      gen_where4string!(query, cargoes::dsl::name, value);
    }
    if let Some(value) = r#where.get("NamespaceName") {
      gen_where4string!(query, cargoes::dsl::namespace_name, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_result::<(CargoDb, CargoSpecDb)>(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      let item = items.0.with_spec(&items.1.try_to_spec()?);
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    let mut query = cargoes::dsl::cargoes
      .inner_join(crate::schema::cargo_specs::table)
      .order(cargoes::dsl::created_at.desc())
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    log::debug!("CargoDb::find filter: {:?}", r#where);
    if let Some(value) = r#where.get("Key") {
      gen_where4string!(query, cargoes::dsl::key, value);
    }
    if let Some(value) = r#where.get("Name") {
      gen_where4string!(query, cargoes::dsl::name, value);
    }
    if let Some(value) = r#where.get("NamespaceName") {
      gen_where4string!(query, cargoes::dsl::namespace_name, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<(CargoDb, CargoSpecDb)>(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      let items = items
        .into_iter()
        .map(|item| {
          let spec = &item.1.try_to_spec()?;
          Ok::<_, IoError>(item.0.with_spec(spec))
        })
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
  }
}

impl CargoDb {
  pub(crate) async fn create_from_spec(
    nsp: &str,
    item: &CargoSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    let nsp = nsp.to_owned();
    let item = item.to_owned();
    let version = version.to_owned();
    // test if the name of the cargo include a . in the name and throw error if true
    if item.name.contains('.') {
      return Err(IoError::invalid_input(
        "CargoSpecPartial",
        "Name cannot contain a dot.",
      ));
    }
    let key = utils::key::gen_key(&nsp, &item.name);
    let new_spec = CargoSpecDb::try_from_spec_partial(&key, &version, &item)?;
    let spec = CargoSpecDb::create(new_spec, pool).await??.try_to_spec()?;
    let new_item = CargoDb {
      key,
      name: item.name,
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: nsp,
      spec_key: spec.key,
    };
    let item = CargoDb::create(new_item, pool).await??;
    let cargo = item.with_spec(&spec);
    Ok(cargo)
  }

  pub(crate) async fn update_from_spec(
    key: &str,
    item: &CargoSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    let version = version.to_owned();
    let cargo = CargoDb::find_by_pk(key, pool).await??;
    let new_spec = CargoSpecDb::try_from_spec_partial(key, &version, item)?;
    let spec = CargoSpecDb::create(new_spec, pool).await??.try_to_spec()?;
    let new_item = CargoUpdateDb {
      name: Some(item.name.to_owned()),
      spec_key: Some(spec.key),
      ..Default::default()
    };
    CargoDb::update_by_pk(key, new_item, pool).await??;
    let cargo = cargo.with_spec(&spec);
    Ok(cargo)
  }

  /// Inspect a cargo item in database for given key
  pub(crate) async fn inspect_by_pk(key: &str, pool: &Pool) -> IoResult<Cargo> {
    use crate::schema::cargo_specs;
    let key = key.to_owned();
    let pool = Arc::clone(pool);
    let item: (CargoDb, CargoSpecDb) = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = cargoes::table
        .inner_join(cargo_specs::table)
        .filter(cargoes::key.eq(key))
        .get_result(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(item)
    })
    .await?;
    let spec = item.1.try_to_spec()?;
    let item = item.0.with_spec(&spec);
    Ok(item)
  }

  pub(crate) async fn find_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Cargo>> {
    let mut r#where = HashMap::new();
    r#where.insert(
      "NamespaceName".to_owned(),
      GenericClause::Eq(name.to_owned()),
    );
    let filter = GenericFilter {
      r#where: Some(r#where),
    };
    CargoDb::find(&filter, pool).await?
  }

  /// Count cargo items in database for given namespace
  pub(crate) async fn count_by_namespace(
    nsp: &str,
    pool: &Pool,
  ) -> IoResult<i64> {
    let nsp = nsp.to_owned();
    let pool = Arc::clone(pool);
    let count = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let count = cargoes::table
        .filter(cargoes::namespace_name.eq(nsp))
        .count()
        .get_result(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(count)
    })
    .await?;
    Ok(count)
  }
}
