use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::generic::GenericDelete;

use crate::utils;
use crate::models::{
  Pool, ContainerInstancePartial, ContainerInstanceDbModel,
  ContainerInstanceUpdateDbModel, ContainerInstance,
};

pub async fn create(
  item: &ContainerInstancePartial,
  pool: &Pool,
) -> IoResult<ContainerInstanceDbModel> {
  let item = ContainerInstanceDbModel::from(item.clone());
  super::generic::insert_with_res(item, pool).await
}

pub async fn update(
  id: &str,
  item: &ContainerInstanceUpdateDbModel,
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

pub async fn find_by_id(key: &str, pool: &Pool) -> IoResult<ContainerInstance> {
  use crate::schema::container_instances;
  let key = key.to_owned();
  let item = super::generic::find_by_id::<
    container_instances::table,
    _,
    ContainerInstanceDbModel,
  >(key, pool)
  .await?;
  let item = ContainerInstance::try_from(item)?;
  Ok(item)
}

pub async fn delete_by_id(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::container_instances;
  let key = key.to_owned();
  super::generic::delete_by_id::<container_instances::table, _>(key, pool).await
}

pub async fn list_by_kind(
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
      .load::<ContainerInstanceDbModel>(&mut conn)
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

pub async fn list_all(pool: &Pool) -> IoResult<Vec<ContainerInstance>> {
  use crate::schema::container_instances;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = container_instances::table
      .load::<ContainerInstanceDbModel>(&mut conn)
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
