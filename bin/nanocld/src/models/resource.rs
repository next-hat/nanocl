use std::sync::Arc;

use diesel::prelude::*;
use diesel::{ExpressionMethods, QueryDsl};
use ntex::web;
use tokio::task::JoinHandle;
use serde::{Serialize, Deserialize};

use nanocl_error::io::{IoResult, IoError, FromIo};

use nanocl_stubs::{
  resource::{Resource, ResourcePartial},
  generic::GenericFilter,
};

use crate::{schema::resources, utils};

use crate::models::resource_spec::ResourceSpecDb;

use super::{WithSpec, Repository, Pool};

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
    unimplemented!()
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
  }
}

impl ResourceDb {
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
    use crate::schema::resource_specs;
    let pk = pk.to_owned();
    let pool = Arc::clone(pool);
    let res: (ResourceDb, ResourceSpecDb) = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let res = resources::table
        .inner_join(resource_specs::table)
        .filter(resources::key.eq(pk))
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| "Resource"))?;
      Ok::<_, IoError>(res)
    })
    .await?;
    let item = res.0.with_spec(&res.1);
    Ok(item)
  }
}
