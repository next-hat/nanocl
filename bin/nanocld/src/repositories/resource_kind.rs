use diesel::prelude::*;

use nanocl_error::{
  io::IoResult,
  http::{HttpError, HttpResult},
};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  resource_kind::{ResourceKind, ResourceKindPartial, ResourceKindInspect},
};

use crate::{
  gen_multiple, gen_where4string,
  schema::resource_kinds,
  models::{Pool, ResourceKindDb, ResourceKindDbUpdate, SpecDb},
};

use super::generic::*;

impl RepositoryBase for ResourceKindDb {}

impl RepositoryCreate for ResourceKindDb {}

impl RepositoryDelByPk for ResourceKindDb {}

impl RepositoryUpdate for ResourceKindDb {
  type UpdateItem = ResourceKindDbUpdate;
}

impl RepositoryReadBy for ResourceKindDb {
  type Output = (ResourceKindDb, SpecDb);

  fn get_pk() -> &'static str {
    "name"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::PgConnection,
    Self::Output,
  > {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::table
      .inner_join(crate::schema::specs::table)
      .into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, resource_kinds::name, value);
    }
    if is_multiple {
      gen_multiple!(query, resource_kinds::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for ResourceKindDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, i64> {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::table.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, resource_kinds::name, value);
    }
    query.count()
  }
}

impl RepositoryReadByTransform for ResourceKindDb {
  type NewOutput = ResourceKind;

  fn transform(item: (ResourceKindDb, SpecDb)) -> IoResult<Self::NewOutput> {
    item.1.try_into()
  }
}

impl ResourceKindDb {
  pub async fn inspect_by_pk(
    pk: &str,
    pool: &Pool,
  ) -> HttpResult<ResourceKindInspect> {
    let item = ResourceKindDb::transform_read_by_pk(pk, pool).await?;
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(item.name.to_owned()));
    let versions = SpecDb::read_by(&filter, pool)
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

  pub async fn create_from_spec(
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
    match ResourceKindDb::transform_read_by_pk(&item.name, pool).await {
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
}
