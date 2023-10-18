use ntex::web;
use diesel::{
  associations, pg, query_dsl, Table, helper_types, query_builder, AsChangeset,
  Insertable,
};
use diesel::RunQueryDsl;

use nanocl_stubs::generic::GenericDelete;
use nanocl_utils::io_error::{IoResult, FromIo};

use crate::utils;
use crate::models::Pool;

pub async fn generic_find_by_id<T, Pk, R>(pool: &Pool, pk: Pk) -> IoResult<R>
where
  T: query_dsl::methods::FindDsl<Pk> + associations::HasTable<Table = T>,
  diesel::dsl::Find<T, Pk>:
    diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, R>,
  Pk: Send + 'static,
  R: Send + 'static,
{
  let pool = pool.clone();
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

pub async fn generic_delete<T, P>(
  pool: &Pool,
  predicate: P,
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
  let pool = pool.clone();
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

pub async fn generic_delete_by_id<T, Pk>(
  pool: &Pool,
  pk: Pk,
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
  let pool = pool.clone();
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

pub async fn generic_update_by_id<T, V, Pk>(
  pool: &Pool,
  pk: Pk,
  values: V,
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
  let pool = pool.clone();

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

pub async fn generic_update_by_id_with_res<T, V, Pk, R>(
  pool: &Pool,
  pk: Pk,
  values: V,
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
  let pool = pool.clone();

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

pub async fn generic_insert_with_res<T, V, R>(
  pool: &Pool,
  values: V,
) -> IoResult<R>
where
  T: associations::HasTable<Table = T> + Table,
  V: Insertable<T>,
  query_builder::InsertStatement<T, <V as Insertable<T>>::Values>:
    query_dsl::LoadQuery<'static, pg::PgConnection, R> + Send,
  R: Send + 'static,
  V: Send + 'static,
{
  let pool = pool.clone();
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
