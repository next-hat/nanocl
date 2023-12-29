use std::sync::Arc;

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
  models::{Pool, ResourceKindDb, ResourceKindDbUpdate, SpecDb},
  schema::resource_kinds,
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

  async fn read_pk_with_spec(pk: &str, pool: &Pool) -> IoResult<Self::Output> {
    log::trace!("CargoDb::find_by_pk: {pk}");
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = resource_kinds::table
        .inner_join(crate::schema::specs::table)
        .filter(resource_kinds::name.eq(pk))
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      item.1.try_into()
    })
    .await?
  }

  async fn read_one_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> IoResult<Self::Output> {
    log::trace!("CargoDb::find_one: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::table
      .inner_join(crate::schema::specs::table)
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
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.1.try_into()?;
      Ok::<_, IoError>(item)
    })
    .await?
  }

  async fn read_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> IoResult<Vec<Self::Output>> {
    log::trace!("CargoDb::find: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::table
      .inner_join(crate::schema::specs::table)
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
        .get_results::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let items = items
        .into_iter()
        .map(|item| item.1.try_into())
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
    .await?
  }
}

impl ResourceKindDb {
  pub(crate) async fn create_from_spec(
    item: &ResourceKindPartial,
    pool: &Pool,
  ) -> HttpResult<ResourceKind> {
    if SpecDb::get_version(&item.name, &item.version, pool)
      .await
      .is_ok()
    {
      return Err(HttpError::conflict(format!(
        "Version {} of {} already exists",
        &item.version, &item.name
      )));
    }
    let kind_version: SpecDb = item.try_into()?;
    let version = SpecDb::create_from(kind_version, pool).await?;
    match ResourceKindDb::read_pk_with_spec(&item.name, pool).await {
      Ok(resource_kind) => {
        let update = ResourceKindDbUpdate {
          spec_key: version.key,
        };
        ResourceKindDb::update_pk(&resource_kind.name, update, pool).await?
      }
      Err(_) => {
        let kind = ResourceKindDb {
          name: item.name.clone(),
          created_at: chrono::Utc::now().naive_utc(),
          spec_key: version.key,
        };
        ResourceKindDb::create_from(kind, pool).await?
      }
    };
    let item: ResourceKind = version.try_into()?;
    Ok(item)
  }

  pub(crate) async fn inspect_by_pk(
    pk: &str,
    pool: &Pool,
  ) -> HttpResult<ResourceKindInspect> {
    let item = ResourceKindDb::read_pk_with_spec(pk, pool).await?;
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(item.name.to_owned()));
    let versions = SpecDb::read(&filter, pool)
      .await?
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
