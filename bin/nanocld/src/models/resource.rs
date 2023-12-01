use std::sync::Arc;

use diesel::prelude::*;
use tokio::task::JoinHandle;
use serde::{Serialize, Deserialize};

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::generic::{GenericFilter, GenericClause};
use nanocl_stubs::resource::{Resource, ResourcePartial};

use crate::{utils, gen_where4string};
use crate::schema::resources;

use super::{Pool, Repository, WithSpec, ResourceSpecDb};

/// This structure represent a resource in the database.
/// A resource is a representation of a specification for internal nanocl services (controllers).
/// Custom `kind` can be added to the system.
/// We use the `spec_key` to link to the resource spec.
/// The `key` is used to identify the resource.
/// The `kind` is used to know which controller to use.
#[derive(
  Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = resources)]
pub struct ResourceDb {
  /// The key of the resource
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The kind of the resource
  pub kind: String,
  /// The spec key reference
  pub spec_key: uuid::Uuid,
}

impl WithSpec for ResourceDb {
  type Type = Resource;
  type Relation = ResourceSpecDb;

  fn with_spec(self, r: &Self::Relation) -> Self::Type {
    Self::Type {
      created_at: self.created_at,
      kind: self.kind,
      spec: r.clone().into(),
    }
  }
}

/// This structure represent the update of a resource in the database.
#[derive(AsChangeset)]
#[diesel(table_name = resources)]
pub struct ResourceUpdateDb {
  /// The key of the resource
  pub key: Option<String>,
  /// The spec key reference
  pub spec_key: Option<uuid::Uuid>,
}

impl Repository for ResourceDb {
  type Table = resources::table;
  type Item = Resource;
  type UpdateItem = ResourceUpdateDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    use crate::schema::resource_specs;
    log::debug!("ResourceDb::find_one filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resources::dsl::resources
      .inner_join(resource_specs::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, resources::dsl::key, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, resources::dsl::kind, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<(ResourceDb, ResourceSpecDb)>(&mut conn)
        .map_err(Self::map_err_context)?;
      let item = item.0.with_spec(&item.1);
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    use crate::schema::resource_specs;
    log::debug!("ResourceDb::find filter: {filter:?}");
    let mut query = resources::dsl::resources
      .order(resources::dsl::created_at.desc())
      .inner_join(resource_specs::table)
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, resources::dsl::key, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, resources::dsl::kind, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<(ResourceDb, ResourceSpecDb)>(&mut conn)
        .map_err(Self::map_err_context)?
        .into_iter()
        .map(|(r, s)| r.with_spec(&s))
        .collect();
      Ok::<_, IoError>(items)
    })
  }
}

impl ResourceDb {
  /// Create a new resource from a spec.
  pub(crate) async fn create_from_spec(
    item: &ResourcePartial,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let spec = ResourceSpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      resource_key: item.name.to_owned(),
      version: item.version.to_owned(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let spec = ResourceSpecDb::create(spec, pool).await??;
    let new_item = ResourceDb {
      key: item.name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      kind: item.kind.clone(),
      spec_key: spec.key.to_owned(),
    };
    let dbmodel = ResourceDb::create(new_item, pool).await??;
    let item = dbmodel.with_spec(&spec);
    Ok(item)
  }

  /// Update a resource from a spec.
  pub(crate) async fn update_from_spec(
    item: &ResourcePartial,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let key = item.name.clone();
    let resource = ResourceDb::inspect_by_pk(&item.name, pool).await?;
    let spec = ResourceSpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      resource_key: resource.spec.resource_key,
      version: item.version.clone(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let spec = ResourceSpecDb::create(spec, pool).await??;
    let resource_update = ResourceUpdateDb {
      key: None,
      spec_key: Some(spec.key.to_owned()),
    };
    let dbmodel =
      ResourceDb::update_by_pk(&key, resource_update, pool).await??;
    let item = dbmodel.with_spec(&spec);
    Ok(item)
  }

  pub(crate) async fn inspect_by_pk(
    pk: &str,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let filter =
      GenericFilter::new().r#where("key", GenericClause::Eq(pk.to_owned()));
    Self::find_one(&filter, pool).await?
  }
}
