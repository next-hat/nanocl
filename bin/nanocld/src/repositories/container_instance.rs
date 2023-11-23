use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::generic::GenericDelete;

use crate::utils;
use crate::models::{
  Pool, ContainerInstancePartial, ContainerInstanceDb,
  ContainerInstanceUpdateDb, ContainerInstance,
};

/// ## Create
///
/// Create a new container instance
///
/// ## Arguments
///
/// * [item](ContainerInstancePartial) - The item to create
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult][IoResult] containing a [ContainerInstanceDb](ContainerInstanceDb)
///
pub(crate) async fn create(
  item: &ContainerInstancePartial,
  pool: &Pool,
) -> IoResult<ContainerInstanceDb> {
  let item = ContainerInstanceDb::from(item.clone());
  super::generic::insert_with_res(item, pool).await
}

/// ## Update
///
/// Update a container instance
///
/// ## Arguments
///
/// * [id](str) - The id of the container instance to update
/// * [item](ContainerInstanceUpdateDb) - The item to update
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult][IoResult] containing a [ContainerInstanceUpdateDb](ContainerInstanceDb)
///
pub(crate) async fn update(
  id: &str,
  item: &ContainerInstanceUpdateDb,
  pool: &Pool,
) -> IoResult<()> {
  use crate::schema::container_instances;
  super::generic::update_by_id::<container_instances::table, _, _>(
    id.to_owned(),
    item.clone(),
    pool,
  )
  .await?;
  Ok(())
}

/// ## Find by id
///
/// Find a container instance by id
///
/// ## Arguments
///
/// * [key](str) - The id of the container instance to find
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult][IoResult] containing a [ContainerInstance](ContainerInstance)
///
pub(crate) async fn find_by_id(
  key: &str,
  pool: &Pool,
) -> IoResult<ContainerInstance> {
  use crate::schema::container_instances;
  let key = key.to_owned();
  let item = super::generic::find_by_id::<
    container_instances::table,
    _,
    ContainerInstanceDb,
  >(key, pool)
  .await?;
  let item = ContainerInstance::try_from(item)?;
  Ok(item)
}

/// ## Delete by id
///
/// Delete a container instance by id
///
/// ## Arguments
///
/// * [key](str) - The id of the container instance to delete
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult][IoResult] containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_id(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::container_instances;
  let key = key.to_owned();
  super::generic::delete_by_id::<container_instances::table, _>(key, pool).await
}

/// ## List for kind
///
/// List container instances for kind and kind id
///
/// ## Arguments
///
/// * [kind](str) - The kind of the container instance to list
/// * [kind_id](str) - The kind id of the container instance to list
///
/// ## Return
///
/// [IoResult][IoResult] containing a [Vec](Vec) of [ContainerInstance](ContainerInstance)
///
pub(crate) async fn list_for_kind(
  kind: &str,
  kind_id: &str,
  pool: &Pool,
) -> IoResult<Vec<ContainerInstance>> {
  use crate::schema::container_instances;
  let pool = pool.clone();
  let kind = kind.to_owned();
  let kind_id = kind_id.to_owned();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = container_instances::table
      .filter(container_instances::kind.eq(kind))
      .filter(container_instances::kind_id.eq(kind_id))
      .load::<ContainerInstanceDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ContainerInstance"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  let items = items
    .into_iter()
    .map(ContainerInstance::try_from)
    .collect::<Result<Vec<ContainerInstance>, IoError>>()?;
  Ok(items)
}

/// ## List all
///
/// List all container instances
///
/// ## Arguments
///
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult][IoResult] containing a [Vec](Vec) of [ContainerInstance](ContainerInstance)
///
pub(crate) async fn list_all(pool: &Pool) -> IoResult<Vec<ContainerInstance>> {
  use crate::schema::container_instances;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = container_instances::table
      .load::<ContainerInstanceDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ContainerInstance"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  let items = items
    .into_iter()
    .map(ContainerInstance::try_from)
    .collect::<Result<Vec<ContainerInstance>, IoError>>()?;
  Ok(items)
}
