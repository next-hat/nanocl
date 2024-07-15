use diesel::{prelude::*, query_dsl::methods::LoadQuery};

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::{GenericClause, GenericFilter};

use crate::{models::Pool, utils};

use super::RepositoryBase;

pub trait RepositoryReadBy: super::RepositoryBase {
  type Output;

  fn get_pk() -> &'static str;

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl LoadQuery<'static, diesel::PgConnection, Self::Output>
  where
    Self::Output: Sized;

  async fn read_one_by(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> IoResult<Self::Output>
  where
    Self::Output: Sized + Send + 'static,
  {
    let pool = pool.clone();
    let filter = filter.clone();
    log::trace!("{}::read_one_by {filter:#?}", Self::get_name());
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let query = Self::gen_read_query(&filter, false);
      let item = query
        .get_result::<Self::Output>(&mut conn)
        .map_err(Self::map_err)?;
      Ok(item)
    })
    .await?
  }

  async fn read_by_pk<Pk>(pk: &Pk, pool: &Pool) -> IoResult<Self::Output>
  where
    Pk: ToString + ?Sized,
    Self::Output: Sized + Send + 'static,
  {
    let pk = pk.to_string();
    log::trace!("{}::read_by_pk {pk}", Self::get_name());
    let filter =
      GenericFilter::new().r#where(Self::get_pk(), GenericClause::Eq(pk));
    Self::read_one_by(&filter, pool).await
  }

  async fn read_by(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> IoResult<Vec<Self::Output>>
  where
    Self::Output: Sized + Send + 'static,
  {
    let pool = pool.clone();
    let filter = filter.clone();
    log::trace!("{}::read_by {filter:#?}", Self::get_name());
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let query = Self::gen_read_query(&filter, true);
      let items = query
        .get_results::<Self::Output>(&mut conn)
        .map_err(Self::map_err)?;
      Ok(items)
    })
    .await?
  }
}

pub trait RepositoryReadByTransform: RepositoryReadBy {
  type NewOutput;

  fn transform(input: Self::Output) -> IoResult<Self::NewOutput>;

  async fn transform_read_by_pk<Pk>(
    pk: &Pk,
    pool: &Pool,
  ) -> IoResult<Self::NewOutput>
  where
    Pk: ToString + ?Sized,
    Self::Output: Sized + Send + 'static,
  {
    let output = Self::read_by_pk(pk, pool).await?;
    Self::transform(output)
  }

  // async fn transform_read_one_by(
  //   filter: &GenericFilter,
  //   pool: &Pool,
  // ) -> IoResult<Self::NewOutput>
  // where
  //   Self::Output: Sized + Send + 'static,
  // {
  //   let output = Self::read_one_by(filter, pool).await?;
  //   Self::transform(output)
  // }

  async fn transform_read_by(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> IoResult<Vec<Self::NewOutput>>
  where
    Self::Output: Sized + Send + 'static,
  {
    Self::read_by(filter, pool)
      .await?
      .into_iter()
      .map(Self::transform)
      .collect()
  }
}

// pub trait RepositoryCountBy

pub trait RepositoryCountBy: RepositoryBase {
  // fn gen_count_query(filter: &GenericFilter)
  //   -> diesel::dsl::Count<Self::Table>;

  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl LoadQuery<'static, diesel::PgConnection, i64>;

  async fn count_by(filter: &GenericFilter, pool: &Pool) -> IoResult<i64> {
    let pool = pool.clone();
    let filter = filter.clone();
    log::trace!("{}::count_by {filter:#?}", Self::get_name());
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let count = Self::gen_count_query(&filter)
        .get_result::<i64>(&mut conn)
        .map_err(Self::map_err)?;
      Ok(count)
    })
    .await?
  }
}
