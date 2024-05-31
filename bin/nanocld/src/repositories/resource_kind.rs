use std::collections::HashMap;

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
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  schema::resource_kinds,
  models::{ColumnType, Pool, ResourceKindDb, ResourceKindDbUpdate, SpecDb},
};

use super::generic::*;

impl RepositoryBase for ResourceKindDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("name", (ColumnType::Text, "resource_kinds.name")),
      (
        "created_at",
        (ColumnType::Timestamptz, "resource_kinds.created_at"),
      ),
      ("spec_key", (ColumnType::Text, "resource_kinds.spec_key")),
    ])
  }
}

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
    let mut query = resource_kinds::table
      .inner_join(crate::schema::specs::table)
      .into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(resource_kinds::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for ResourceKindDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, i64> {
    let mut query = resource_kinds::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
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
    let filter: GenericFilter = GenericFilter::new()
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
