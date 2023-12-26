use std::sync::Arc;

use ntex::rt::JoinHandle;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  resource::Resource,
  resource::ResourcePartial,
};

use crate::{
  utils, gen_where4string, gen_where4json,
  models::{Pool, ResourceDb, ResourceUpdateDb, ResourceKindDb, SpecDb},
  schema::{resources, specs},
};

use super::generic::*;

impl RepositoryBase for ResourceDb {}

impl RepositoryCreate for ResourceDb {}

impl RepositoryUpdate for ResourceDb {
  type UpdateItem = ResourceUpdateDb;
}

impl RepositoryDelByPk for ResourceDb {}

impl RepositoryReadWithSpec for ResourceDb {
  type Output = Resource;

  fn read_pk_with_spec(
    pk: &str,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("ResourceDb::find_by_pk: {pk}");
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = resources::dsl::resources
        .inner_join(crate::schema::specs::table)
        .filter(resources::dsl::key.eq(pk))
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.0.with_spec(&item.1);
      Ok::<_, IoError>(item)
    })
  }

  fn read_one_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("ResourceDb::find_one: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resources::dsl::resources
      .inner_join(specs::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, resources::dsl::key, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, resources::dsl::kind, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, specs::dsl::data, value);
    }
    if let Some(value) = r#where.get("metadata") {
      gen_where4json!(query, specs::dsl::metadata, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.0.with_spec(&item.1);
      Ok::<_, IoError>(item)
    })
  }

  fn read_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Output>>> {
    log::trace!("ResourceDb::find: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resources::dsl::resources
      .inner_join(specs::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, resources::dsl::key, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, resources::dsl::kind, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, specs::dsl::data, value);
    }
    if let Some(value) = r#where.get("metadata") {
      gen_where4json!(query, specs::dsl::metadata, value);
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
        .map(|item| item.0.with_spec(&item.1))
        .collect::<Vec<_>>();
      Ok::<_, IoError>(items)
    })
  }
}

impl WithSpec for ResourceDb {
  type Output = Resource;
  type Relation = SpecDb;

  fn with_spec(self, r: &Self::Relation) -> Self::Output {
    Self::Output {
      created_at: self.created_at,
      kind: self.kind,
      spec: r.clone().into(),
    }
  }
}

impl ResourceDb {
  pub(crate) async fn parse_kind(
    kind: &str,
    pool: &Pool,
  ) -> IoResult<(String, String)> {
    let items = kind.split('/').collect::<Vec<_>>();
    match items.get(2) {
      Some(version) => {
        Ok((items[..2].join("/"), version.to_owned().to_string()))
      }
      None => {
        let kind = ResourceKindDb::read_pk_with_spec(kind, pool).await??;
        Ok((kind.name, kind.version))
      }
    }
  }

  /// Create a new resource from a spec.
  pub(crate) async fn create_from_spec(
    item: &ResourcePartial,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let (kind, version) = ResourceDb::parse_kind(&item.kind, pool).await?;
    let spec = SpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: "Resource".to_owned(),
      kind_key: item.name.to_owned(),
      version: version.to_owned(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let spec = SpecDb::create_from(spec, pool).await??;
    let new_item = ResourceDb {
      key: item.name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      kind,
      spec_key: spec.key.to_owned(),
    };
    let dbmodel = ResourceDb::create_from(new_item, pool).await??;
    let item = dbmodel.with_spec(&spec);
    Ok(item)
  }

  /// Update a resource from a spec.
  pub(crate) async fn update_from_spec(
    item: &ResourcePartial,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let key = item.name.clone();
    let resource = ResourceDb::read_pk_with_spec(&item.name, pool).await??;
    let (_, version) = ResourceDb::parse_kind(&item.kind, pool).await?;
    let spec = SpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: "Resource".to_owned(),
      kind_key: resource.spec.resource_key,
      version: version.clone(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let spec = SpecDb::create_from(spec, pool).await??;
    let resource_update = ResourceUpdateDb {
      key: None,
      spec_key: Some(spec.key.to_owned()),
    };
    let dbmodel = ResourceDb::update_pk(&key, resource_update, pool).await??;
    let item = dbmodel.with_spec(&spec);
    Ok(item)
  }

  pub(crate) async fn inspect_by_pk(
    pk: &str,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let filter =
      GenericFilter::new().r#where("key", GenericClause::Eq(pk.to_owned()));
    Self::read_one_with_spec(&filter, pool).await?
  }
}
