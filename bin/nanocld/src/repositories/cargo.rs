use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use nanocl_stubs::{generic, cargo, cargo_config};

use crate::{utils, models};

/// ## Find by namespace
///
/// Find a cargo by a `models::NamespaceDbModel` in database and return a `Vec<models::CargoDbModel>`.
///
/// ## Arguments
///
/// - [nsp](models::NamespaceDbModel) - Namespace item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Vec<models::CargoDbModel>) - List a cargo found
///  - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_namespace(
  nsp: &models::NamespaceDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::CargoDbModel>> {
  let query = cargo::GenericCargoListQuery::of_namespace(nsp.clone());
  list_by_query(&query, pool).await
}

/// ## List by query
///
/// List a cargo by a `cargo::GenericCargoListQuery` in database and return a `Vec<models::CargoDbModel>`.
///
/// ## Arguments
///
/// - [query](cargo::GenericCargoListQuery) - Query containing namespace, name filter and pagination info
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Vec<models::CargoDbModel>) - List a cargo found
///  - [Err](io_error::IoError) - Error during the operation
///
pub async fn list_by_query(
  query: &cargo::GenericCargoListQuery<models::NamespaceDbModel>,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::CargoDbModel>> {
  use crate::schema::cargoes::dsl;
  let query = query.clone();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut sql =
      models::CargoDbModel::belonging_to(&query.namespace).into_boxed();
    if let Some(name) = &query.name {
      sql = sql.filter(dsl::name.ilike(format!("%{name}%")));
    }
    if let Some(limit) = query.limit {
      sql = sql.limit(limit);
    }
    if let Some(offset) = query.offset {
      sql = sql.offset(offset);
    }
    let items = sql
      .order(dsl::created_at.desc())
      .get_results(&mut conn)
      .map_err(|err| err.map_err_context(|| "cargo::Cargo"))?;
    Ok::<_, io_error::IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Create
///
/// Create a cargo item in database for given namespace
///
/// ## Arguments
///
/// - [nsp](str) - Namespace name
/// - [item](cargo::Cargo) - cargo::Cargo item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](cargo::Cargo) - The cargo created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  nsp: &str,
  item: &cargo_config::CargoConfigPartial,
  version: &str,
  pool: &models::Pool,
) -> io_error::IoResult<cargo::Cargo> {
  let nsp = nsp.to_owned();
  let item = item.to_owned();
  let version = version.to_owned();
  // test if the name of the cargo include a . in the name and throw error if true
  if item.name.contains('.') {
    return Err(io_error::IoError::invalid_input(
      "cargo_config::CargoConfigPartial",
      "Name cannot contain a dot.",
    ));
  }
  let key = utils::key::gen_key(&nsp, &item.name);
  let config = super::cargo_config::create(&key, &item, &version, pool).await?;
  let new_item = models::CargoDbModel {
    key,
    name: item.name,
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    config_key: config.key,
  };

  let item: models::CargoDbModel =
    utils::repository::generic_insert_with_res(pool, new_item).await?;

  let cargo = item.into_cargo(config);
  Ok(cargo)
}

/// ## Delete by key
///
/// Delete a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](str) - cargo::Cargo key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](generic::GenericDelete) - The number of deleted items
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let key = key.to_owned();

  utils::repository::generic_delete_by_id::<crate::schema::cargoes::table, _>(
    pool,
    key.to_owned(),
  )
  .await
}

/// ## Find by key
///
/// Find a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](str) - cargo::Cargo key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::CargoDbModel) - The cargo found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<models::CargoDbModel> {
  let key = key.to_owned();

  utils::repository::generic_find_by_id::<crate::schema::cargoes::table, _, _>(
    pool,
    key.to_owned(),
  )
  .await
}
/// ## Update by key
///
/// Update a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](str) - cargo::Cargo key
/// - [item](cargo_config::CargoConfigPartial) - cargo::Cargo config
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](cargo::Cargo) - The cargo updated
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn update_by_key(
  key: &str,
  item: &cargo_config::CargoConfigPartial,
  version: &str,
  pool: &models::Pool,
) -> io_error::IoResult<cargo::Cargo> {
  let version = version.to_owned();
  let cargodb = find_by_key(key, pool).await?;
  let config = super::cargo_config::create(key, item, &version, pool).await?;
  let new_item = models::CargoUpdateDbModel {
    name: Some(item.name.to_owned()),
    config_key: Some(config.key),
    ..Default::default()
  };

  let key = key.to_owned();

  utils::repository::generic_update_by_id::<
    crate::schema::cargoes::table,
    models::CargoUpdateDbModel,
    _,
  >(pool, key.to_owned(), new_item)
  .await?;

  let cargo = cargodb.into_cargo(config);
  Ok(cargo)
}

/// ## Count by namespace
///
/// Count cargo items in database for given namespace
///
/// ## Arguments
///
/// - [namespace](str) - Namespace name
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](i64) - The number of cargo items
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn count_by_namespace(
  nsp: &str,
  pool: &models::Pool,
) -> io_error::IoResult<i64> {
  use crate::schema::cargoes;

  let nsp = nsp.to_owned();
  let pool = pool.clone();
  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let count = cargoes::table
      .filter(cargoes::namespace_name.eq(nsp))
      .count()
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "cargo::Cargo"))?;
    Ok::<_, io_error::IoError>(count)
  })
  .await?;
  Ok(count)
}

/// ## Inspect by key
///
/// Inspect a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](str) - cargo::Cargo key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](cargo::Cargo) - The cargo found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn inspect_by_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<cargo::Cargo> {
  use crate::schema::cargoes;
  use crate::schema::cargo_configs;
  let key = key.to_owned();
  let pool = pool.clone();
  let item: (models::CargoDbModel, models::CargoConfigDbModel) =
    web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = cargoes::table
        .inner_join(cargo_configs::table)
        .filter(cargoes::key.eq(key))
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| "cargo::Cargo"))?;
      Ok::<_, io_error::IoError>(item)
    })
    .await?;
  let config = serde_json::from_value::<cargo_config::CargoConfigPartial>(
    item.1.data.clone(),
  )
  .map_err(|err| err.map_err_context(|| "cargo_config::CargoConfigPartial"))?;

  let config = item.1.into_cargo_config(&config);

  let item = item.0.into_cargo(config);
  Ok(item)
}
