use std::sync::Arc;

use ntex::rt::JoinHandle;
use diesel::{prelude::*, associations::HasTable};
use diesel::internal::table_macro::BoxedSelectStatement;

use nanocl_error::io::{IoResult, IoError};

use nanocl_stubs::generic::GenericFilter;

use crate::{utils, models::Pool};

pub trait RepositoryRead: super::RepositoryBase {
  type Output;

  fn read_by_pk<Pk>(pk: &Pk, pool: &Pool) -> JoinHandle<IoResult<Self>>
  where
    Self: Sized + Send + HasTable + 'static,
    Pk: ToOwned + ?Sized + std::fmt::Display,
    <Pk as ToOwned>::Owned: Send + 'static,
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
      Ok(item)
    })
  }

  fn gen_read_query(
    filter: &GenericFilter,
  ) -> BoxedSelectStatement<
    'static,
    diesel::helper_types::SqlTypeOf<<Self as HasTable>::Table>,
    diesel::internal::table_macro::FromClause<<Self as HasTable>::Table>,
    diesel::pg::Pg,
  >
  where
    Self: Sized
      + HasTable
      + diesel::Selectable<diesel::pg::Pg>
      + diesel::Queryable<Self, diesel::pg::Pg>,
    <Self as HasTable>::Table:
      diesel::query_builder::QueryFragment<diesel::pg::Pg>,
    <Self as diesel::Selectable<diesel::pg::Pg>>::SelectExpression:
      diesel::query_builder::QueryId,
    <Self as HasTable>::Table: diesel::Expression;

  fn read(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Output>>>
  where
    Self: Sized
      + Send
      + HasTable
      + diesel::Selectable<diesel::pg::Pg>
      + diesel::Queryable<Self, diesel::pg::Pg>
      + 'static,
    Self::Output: Send + TryFrom<Self> + 'static,
    <Self::Output as TryFrom<Self>>::Error: std::fmt::Display,
    <Self as HasTable>::Table: diesel::Table
      + diesel::Expression
      + diesel::query_builder::QueryFragment<diesel::pg::Pg>,
    diesel::pg::Pg: diesel::sql_types::HasSqlType<Self>,
    Self: diesel::Queryable<diesel::sql_types::Bool, diesel::pg::Pg>,
    <<Self as HasTable>::Table as diesel::QuerySource>::FromClause:
      diesel::query_builder::QueryFragment<diesel::pg::Pg>,
    Self: diesel::Selectable<diesel::pg::Pg>,
    <Self as diesel::Selectable<diesel::pg::Pg>>::SelectExpression:
      diesel::query_builder::QueryId,
    Self: diesel::Table,
    <<Self as HasTable>::Table as diesel::Expression>::SqlType:
      diesel::sql_types::SingleValue,
    Self: diesel::Queryable<
      <<Self as HasTable>::Table as diesel::Expression>::SqlType,
      diesel::pg::Pg,
    >,
    diesel::pg::Pg: diesel::sql_types::HasSqlType<
      <<Self as HasTable>::Table as diesel::Expression>::SqlType,
    >, // diesel::query_dsl::LoadQuery<'_, _, Self>,
  {
    log::trace!("{}::read: {filter:?}", Self::get_name());
    let pool = Arc::clone(pool);
    let filter = filter.clone();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let query = Self::gen_read_query(&filter);
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
}
