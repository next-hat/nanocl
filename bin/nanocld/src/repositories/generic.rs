use std::sync::Arc;

use ntex::web;
use diesel::{
  associations, pg, query_dsl, Table, helper_types, query_builder, AsChangeset,
  Insertable, RunQueryDsl,
};

use nanocl_error::io::{IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;

use crate::utils;
use crate::models::Pool;

/// ## Find by id
///
/// Find a row in the table by its primary key
///
/// ## Arguments
///
/// * [pk](Send) - The primary key of the row to find
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [R](Send)
///
pub(crate) async fn find_by_id<T, Pk, R>(pk: Pk, pool: &Pool) -> IoResult<R>
where
  T: query_dsl::methods::FindDsl<Pk> + associations::HasTable<Table = T>,
  diesel::dsl::Find<T, Pk>:
    diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, R>,
  Pk: Send + 'static,
  R: Send + 'static,
{
  let pool = Arc::clone(pool);
  let item = web::block(move || {
    let query = <T as associations::HasTable>::table().find(pk);
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = query
      .get_result::<R>(&mut conn)
      .map_err(|err| err.map_err_context(|| std::any::type_name::<R>()))?;
    Ok(res)
  })
  .await?;
  Ok(item)
}

/// ## Delete
///
/// Delete a row in the table
///
/// ## Arguments
///
/// * [predicate](Send) - The predicate to filter the row to delete
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete<T, P>(
  predicate: P,
  pool: &Pool,
) -> IoResult<GenericDelete>
where
  T: query_dsl::methods::FilterDsl<P> + associations::HasTable<Table = T>,
  helper_types::Filter<T, P>: query_builder::IntoUpdateTarget,
  query_builder::DeleteStatement<
    <helper_types::Filter<T, P> as associations::HasTable>::Table,
    <helper_types::Filter<T, P> as query_builder::IntoUpdateTarget>::WhereClause,
  >: query_builder::QueryFragment<pg::Pg> + query_builder::QueryId,
  P: Send + 'static,
{
  let pool = Arc::clone(pool);
  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item =
      diesel::delete(<T as associations::HasTable>::table().filter(predicate))
        .execute(&mut conn)
        .map_err(|err| err.map_err_context(|| std::any::type_name::<T>()))?;
    Ok(item)
  })
  .await?;
  Ok(GenericDelete { count })
}

/// ## Delete by id
///
/// Delete a row in the table
///
/// ## Arguments
///
/// * [pk](Send) - The primary key of the row to delete
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_id<T, Pk>(
  pk: Pk,
  pool: &Pool,
) -> IoResult<GenericDelete>
where
  T: query_dsl::methods::FindDsl<Pk> + associations::HasTable<Table = T>,
  helper_types::Find<T, Pk>: query_builder::IntoUpdateTarget,
  Pk: Send + 'static,
  query_builder::DeleteStatement<
    <helper_types::Find<T, Pk> as associations::HasTable>::Table,
    <helper_types::Find<T, Pk> as query_builder::IntoUpdateTarget>::WhereClause,
  >: query_builder::QueryFragment<pg::Pg> + query_builder::QueryId,
{
  let pool = Arc::clone(pool);
  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = diesel::delete(<T as associations::HasTable>::table().find(pk))
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| std::any::type_name::<T>()))?;
    Ok(item)
  })
  .await?;
  Ok(GenericDelete { count })
}

/// ## Update by id
///
/// Update a row in the table
///
/// ## Arguments
///
/// * [pk](Send) - The primary key of the row to update
/// * [values](AsChangeset) - The values to update
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [usize](usize)
///
pub(crate) async fn update_by_id<T, V, Pk>(
  pk: Pk,
  values: V,
  pool: &Pool,
) -> IoResult<usize>
where
  T: query_dsl::methods::FindDsl<Pk> + associations::HasTable<Table = T>,
  V: AsChangeset<
      Target = <helper_types::Find<T, Pk> as associations::HasTable>::Table,
    > + Send
    + 'static,
  helper_types::Find<T, Pk>: query_builder::IntoUpdateTarget,
  query_builder::UpdateStatement<
    <helper_types::Find<T, Pk> as associations::HasTable>::Table,
    <helper_types::Find<T, Pk> as query_builder::IntoUpdateTarget>::WhereClause,
    <V as AsChangeset>::Changeset,
  >: query_builder::AsQuery + query_builder::QueryFragment<pg::Pg>,
  helper_types::Find<T, Pk>: query_dsl::methods::LimitDsl,
  Pk: Send + 'static,
{
  let pool = Arc::clone(pool);
  let res = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::update(<T as associations::HasTable>::table().find(pk))
      .set(values)
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| std::any::type_name::<T>()))?;
    Ok(res)
  })
  .await?;
  Ok(res)
}

/// ## Update by id with res
///
/// Update a row in the table and return the updated item
///
/// ## Arguments
///
/// * [pk](Send) - The primary key of the row to update
/// * [values](AsChangeset) - The values to update
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [R](Send)
///
pub(crate) async fn update_by_id_with_res<T, V, Pk, R>(
  pk: Pk,
  values: V,
  pool: &Pool,
) -> IoResult<R>
where
  T: query_dsl::methods::FindDsl<Pk> + associations::HasTable<Table = T>,
  V: AsChangeset<
      Target = <helper_types::Find<T, Pk> as associations::HasTable>::Table,
    > + Send
    + 'static,
  helper_types::Find<T, Pk>: query_builder::IntoUpdateTarget,
  query_builder::UpdateStatement<
    <helper_types::Find<T, Pk> as associations::HasTable>::Table,
    <helper_types::Find<T, Pk> as query_builder::IntoUpdateTarget>::WhereClause,
    <V as AsChangeset>::Changeset,
  >:
    query_builder::AsQuery + query_dsl::LoadQuery<'static, pg::PgConnection, R>,
  Pk: Send + 'static,
  R: Send + 'static,
{
  let pool = Arc::clone(pool);
  let res = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::update(<T as associations::HasTable>::table().find(pk))
      .set(values)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| std::any::type_name::<T>()))?;
    Ok(res)
  })
  .await?;
  Ok(res)
}

/// ## insert with res
///
/// Insert a new row into the table and return the inserted item
///
/// ## Arguments
///
/// * [value](Insertable) - The value to insert
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [R](Send)
///
pub(crate) async fn insert_with_res<T, V, R>(
  values: V,
  pool: &Pool,
) -> IoResult<R>
where
  T: associations::HasTable<Table = T> + Table,
  V: Insertable<T>,
  query_builder::InsertStatement<T, <V as Insertable<T>>::Values>:
    query_dsl::LoadQuery<'static, pg::PgConnection, R> + Send,
  R: Send + 'static,
  V: Send + 'static,
{
  let pool = Arc::clone(pool);
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::insert_into(<T as associations::HasTable>::table())
      .values(values)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| std::any::type_name::<T>()))?;
    Ok(res)
  })
  .await?;
  Ok(item)
}
