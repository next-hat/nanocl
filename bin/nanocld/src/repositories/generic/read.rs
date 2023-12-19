use std::sync::Arc;

use ntex::rt::JoinHandle;
use diesel::{prelude::*, associations::HasTable, query_dsl::methods::LoadQuery};

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::generic::GenericFilter;

use crate::{utils, models::Pool};

pub trait RepositoryRead: super::RepositoryBase {
  type Output;
  type Query;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query;

  fn read_by_pk<Pk>(pk: &Pk, pool: &Pool) -> JoinHandle<IoResult<Self::Output>>
  where
    Pk: ToOwned + ?Sized + std::fmt::Display,
    <Pk as ToOwned>::Owned: Send + 'static,
    Self: Sized + Send + HasTable + 'static,
    Self::Output: Send + TryFrom<Self>,
    <Self::Output as TryFrom<Self>>::Error: std::fmt::Display,
    Self::Table: diesel::query_dsl::methods::FindDsl<<Pk as ToOwned>::Owned>
      + HasTable<Table = Self::Table>,
    diesel::dsl::Find<Self::Table, <Pk as ToOwned>::Owned>:
      diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, Self>,
  {
    log::trace!("{}::find_by_pk: {pk}", Self::get_name());
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let query = <Self::Table as HasTable>::table().find(pk);
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query.get_result::<Self>(&mut conn).map_err(Self::map_err)?;
      Self::Output::try_from(item).map_err(|err| {
        IoError::invalid_data(Self::get_name(), &err.to_string())
      })
    })
  }

  fn read(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Output>>>
  where
    Self: Sized + Send + HasTable + 'static,
    Self::Output: Send + TryFrom<Self>,
    <Self::Output as TryFrom<Self>>::Error: std::fmt::Display,
    Self::Query: LoadQuery<'static, diesel::PgConnection, Self>,
  {
    log::trace!("{}::read: {filter:?}", Self::get_name());
    let pool = Arc::clone(pool);
    let filter = filter.clone();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let query = Self::gen_read_query(&filter, true);
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err)?
        .into_iter()
        .map(|i| {
          Self::Output::try_from(i).map_err(|err| {
            IoError::invalid_data(Self::get_name(), &err.to_string())
          })
        })
        .collect::<IoResult<Vec<Self::Output>>>()?;
      Ok(items)
    })
  }

  fn read_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>>
  where
    Self: Sized + Send + HasTable + 'static,
    Self::Output: Send + TryFrom<Self>,
    <Self::Output as TryFrom<Self>>::Error: std::fmt::Display,
    Self::Query: LoadQuery<'static, diesel::PgConnection, Self>,
  {
    log::trace!("{}::read_one: {filter:?}", Self::get_name());
    let pool = Arc::clone(pool);
    let filter = filter.clone();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let query = Self::gen_read_query(&filter, false);
      let item = query.get_result::<Self>(&mut conn).map_err(Self::map_err)?;
      Self::Output::try_from(item).map_err(|err| {
        IoError::invalid_data(Self::get_name(), &err.to_string())
      })
    })
  }
}

pub trait RepositoryReadWithSpec: super::RepositoryBase {
  type Output;

  fn read_pk_with_spec(
    filter: &str,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>>;

  fn read_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Output>>>;

  fn read_one_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>>;
}
