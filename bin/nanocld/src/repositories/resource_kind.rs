use std::sync::Arc;

use ntex::rt::JoinHandle;
use diesel::prelude::*;

use nanocl_error::{
  io::{IoError, IoResult},
  http::{HttpError, HttpResult},
};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  resource_kind::{ResourceKind, ResourceKindPartial, ResourceKindInspect},
};

use crate::{
  utils, gen_where4string,
  models::{Pool, ResourceKindDb, ResourceKindVersionDb, ResourceKindDbUpdate},
  schema::{resource_kinds, resource_kind_versions},
};

use super::generic::*;

impl RepositoryBase for ResourceKindDb {}

impl RepositoryCreate for ResourceKindDb {}

impl RepositoryDelByPk for ResourceKindDb {}

impl RepositoryUpdate for ResourceKindDb {
  type UpdateItem = ResourceKindDbUpdate;
}

impl RepositoryReadWithSpec for ResourceKindDb {
  type Output = ResourceKind;

  fn read_pk_with_spec(
    pk: &str,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("CargoDb::find_by_pk: {pk}");
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = resource_kinds::table
        .inner_join(crate::schema::resource_kind_versions::table)
        .filter(resource_kinds::name.eq(pk))
        .get_result::<(Self, ResourceKindVersionDb)>(&mut conn)
        .map_err(Self::map_err)?;
      item.1.try_into()
    })
  }

  fn read_one_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("CargoDb::find_one: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::table
      .inner_join(crate::schema::resource_kind_versions::table)
      .into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, resource_kinds::name, value);
    }
    // if let Some(value) = r#where.get("version_key") {
    //   gen_where4string!(query, resource_kinds::version_key, value);
    // }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<(Self, ResourceKindVersionDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.1.try_into()?;
      Ok::<_, IoError>(item)
    })
  }

  fn read_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Output>>> {
    log::trace!("CargoDb::find: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::table
      .inner_join(crate::schema::resource_kind_versions::table)
      .order(resource_kinds::created_at.desc())
      .into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, resource_kinds::name, value);
    }
    // if let Some(value) = r#where.get("version_key") {
    //   gen_where4string!(query, resource_kinds::dsl::version_key, value);
    // }
    let limit = filter.limit.unwrap_or(100);
    query = query.limit(limit as i64);
    if let Some(offset) = filter.offset {
      query = query.offset(offset as i64);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<(Self, ResourceKindVersionDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let items = items
        .into_iter()
        .map(|item| item.1.try_into())
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
  }
}

impl ResourceKindDb {
  pub(crate) async fn create_from_spec(
    item: &ResourceKindPartial,
    pool: &Pool,
  ) -> HttpResult<ResourceKind> {
    if ResourceKindVersionDb::get_version(&item.name, &item.version, pool)
      .await
      .is_ok()
    {
      return Err(HttpError::conflict(format!(
        "Version {} of {} already exists",
        &item.version, &item.name
      )));
    }
    let kind_version: ResourceKindVersionDb = item.try_into()?;
    let version =
      ResourceKindVersionDb::create_from(kind_version, pool).await??;
    match ResourceKindDb::read_pk_with_spec(&item.name, pool).await? {
      Ok(resource_kind) => {
        let update = ResourceKindDbUpdate {
          version_key: version.key,
        };
        ResourceKindDb::update_pk(&resource_kind.name, update, pool).await??
      }
      Err(_) => {
        let kind = ResourceKindDb {
          name: item.name.clone(),
          created_at: chrono::Utc::now().naive_utc(),
          version_key: version.key,
        };
        ResourceKindDb::create_from(kind, pool).await??
      }
    };
    let item: ResourceKind = version.try_into()?;
    Ok(item)
  }

  pub(crate) async fn inspect_by_pk(
    pk: &str,
    pool: &Pool,
  ) -> HttpResult<ResourceKindInspect> {
    let item = ResourceKindDb::read_pk_with_spec(pk, pool).await??;
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(item.name.to_owned()));
    let versions = ResourceKindVersionDb::read(&filter, pool)
      .await??
      .into_iter()
      .map(|item| item.try_into())
      .collect::<IoResult<Vec<_>>>()?;
    let item = ResourceKindInspect {
      name: item.name,
      created_at: item.created_at,
      versions,
    };
    Ok(item)
  }
}

impl RepositoryBase for ResourceKindVersionDb {}

impl RepositoryCreate for ResourceKindVersionDb {}

impl RepositoryRead for ResourceKindVersionDb {
  type Output = ResourceKindVersionDb;
  type Query = resource_kind_versions::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query =
      resource_kind_versions::dsl::resource_kind_versions.into_boxed();
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, resource_kind_versions::kind_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_kind_versions::dsl::version, value);
    }
    if is_multiple {
      query = query.order(resource_kind_versions::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl RepositoryDelBy for ResourceKindVersionDb {
  fn gen_del_query(
    filter: &GenericFilter,
  ) -> diesel::query_builder::BoxedDeleteStatement<
    'static,
    diesel::pg::Pg,
    <Self as diesel::associations::HasTable>::Table,
  >
  where
    Self: diesel::associations::HasTable,
  {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = diesel::delete(resource_kind_versions::table).into_boxed();
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, resource_kind_versions::kind_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_kind_versions::dsl::version, value);
    }
    query
  }
}

impl ResourceKindVersionDb {
  pub(crate) async fn get_version(
    name: &str,
    version: &str,
    pool: &Pool,
  ) -> HttpResult<ResourceKindVersionDb> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(name.to_owned()))
      .r#where("version", GenericClause::Eq(version.to_owned()));
    let item = ResourceKindVersionDb::read_one(&filter, pool).await??;
    Ok(item)
  }
}
