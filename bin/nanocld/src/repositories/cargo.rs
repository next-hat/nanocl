use std::sync::Arc;

use ntex::rt::JoinHandle;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  cargo::Cargo,
  cargo_spec::{CargoSpecPartial, CargoSpec},
};

use crate::{
  utils, gen_where4string,
  models::{Pool, CargoDb, SpecDb, CargoUpdateDb},
  schema::cargoes,
};

use super::generic::*;

impl RepositoryBase for CargoDb {}

impl RepositoryCreate for CargoDb {}

impl RepositoryUpdate for CargoDb {
  type UpdateItem = CargoUpdateDb;
}

impl RepositoryDelByPk for CargoDb {}

impl RepositoryReadWithSpec for CargoDb {
  type Output = Cargo;

  fn read_pk_with_spec(
    pk: &str,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("CargoDb::find_by_pk: {pk}");
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = cargoes::dsl::cargoes
        .inner_join(crate::schema::specs::table)
        .filter(cargoes::dsl::key.eq(pk))
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.0.with_spec(&item.1.try_to_cargo_spec()?);
      Ok::<_, IoError>(item)
    })
  }

  fn read_one_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("CargoDb::find_one: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = cargoes::dsl::cargoes
      .inner_join(crate::schema::specs::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, cargoes::dsl::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, cargoes::dsl::name, value);
    }
    if let Some(value) = r#where.get("namespace_name") {
      gen_where4string!(query, cargoes::dsl::namespace_name, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.0.with_spec(&item.1.try_to_cargo_spec()?);
      Ok::<_, IoError>(item)
    })
  }

  fn read_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Output>>> {
    log::trace!("CargoDb::find: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = cargoes::dsl::cargoes
      .inner_join(crate::schema::specs::table)
      .order(cargoes::dsl::created_at.desc())
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, cargoes::dsl::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, cargoes::dsl::name, value);
    }
    if let Some(value) = r#where.get("namespace_name") {
      gen_where4string!(query, cargoes::dsl::namespace_name, value);
    }
    let limit = filter.limit.unwrap_or(100);
    query = query.limit(limit as i64);
    if let Some(offset) = filter.offset {
      query = query.offset(offset as i64);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let items = items
        .into_iter()
        .map(|item| {
          let spec = &item.1.try_to_cargo_spec()?;
          Ok::<_, IoError>(item.0.with_spec(spec))
        })
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
  }
}

impl WithSpec for CargoDb {
  type Output = Cargo;
  type Relation = CargoSpec;

  fn with_spec(self, r: &Self::Relation) -> Self::Output {
    Self::Output {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.clone(),
    }
  }
}

impl CargoDb {
  /// Create a new cargo from its specification.
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
    let new_spec = SpecDb::try_from_cargo_partial(&key, &version, &item)?;
    let spec = SpecDb::create_from(new_spec, pool)
      .await??
      .try_to_cargo_spec()?;
    let new_item = CargoDb {
      key,
      name: item.name,
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: nsp,
      spec_key: spec.key,
    };
    let item = Self::create_from(new_item, pool).await??.with_spec(&spec);
    Ok(item)
  }

  /// Update a cargo from its specification.
  pub(crate) async fn update_from_spec(
    key: &str,
    item: &CargoSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    let version = version.to_owned();
    let mut cargo = CargoDb::read_pk_with_spec(key, pool).await??;
    let new_spec = SpecDb::try_from_cargo_partial(key, &version, item)?;
    let spec = SpecDb::create_from(new_spec, pool)
      .await??
      .try_to_cargo_spec()?;
    let new_item = CargoUpdateDb {
      name: Some(item.name.to_owned()),
      spec_key: Some(spec.key),
      ..Default::default()
    };
    Self::update_pk(key, new_item, pool).await??;
    cargo.spec = spec;
    Ok(cargo)
  }

  /// Find a cargo by its key.
  pub(crate) async fn inspect_by_pk(key: &str, pool: &Pool) -> IoResult<Cargo> {
    let filter =
      GenericFilter::new().r#where("key", GenericClause::Eq(key.to_owned()));
    Self::read_one_with_spec(&filter, pool).await?
  }

  /// Find cargoes by namespace.
  pub(crate) async fn find_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Cargo>> {
    let filter = GenericFilter::new()
      .r#where("namespace_name", GenericClause::Eq(name.to_owned()));
    Self::read_with_spec(&filter, pool).await?
  }

  /// Count cargoes by namespace.
  pub(crate) async fn count_by_namespace(
    nsp: &str,
    pool: &Pool,
  ) -> IoResult<i64> {
    let nsp = nsp.to_owned();
    let pool = Arc::clone(pool);
    let count = ntex::web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let count = cargoes::table
        .filter(cargoes::namespace_name.eq(nsp))
        .count()
        .get_result(&mut conn)
        .map_err(Self::map_err)?;
      Ok::<_, IoError>(count)
    })
    .await?;
    Ok(count)
  }
}
