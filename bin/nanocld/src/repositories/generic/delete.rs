use diesel::{associations::HasTable, prelude::*, query_builder, query_dsl};

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::GenericFilter;

use crate::{models::Pool, utils};

pub trait RepositoryDelByPk: super::RepositoryBase {
  async fn del_by_pk<Pk>(
    pk: &Pk,
    pool: &Pool,
  ) -> IoResult<()>
  where
    Self: Sized + HasTable,
    Pk: ToOwned + ?Sized + std::fmt::Display,
    <Pk as ToOwned>::Owned: Send + 'static,
    Self::Table: query_dsl::methods::FindDsl<<Pk as ToOwned>::Owned> + HasTable<Table = Self::Table>,
    diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned>: query_builder::IntoUpdateTarget,
    query_builder::DeleteStatement<
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as HasTable>::Table,
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as query_builder::IntoUpdateTarget>::WhereClause,
    >: query_builder::QueryFragment<diesel::pg::Pg> + query_builder::QueryId,
  {
    log::trace!("{}::delete_by_pk: {pk}", Self::get_name());
    let pool = pool.clone();
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      diesel::delete(<Self::Table as HasTable>::table().find(pk))
        .execute(&mut conn)
        .map_err(Self::map_err)?;
      Ok(())
    })
    .await?
  }
}

pub trait RepositoryDelBy: super::RepositoryBase {
  fn gen_del_query(
    filter: &GenericFilter,
  ) -> diesel::query_builder::BoxedDeleteStatement<
    'static,
    diesel::pg::Pg,
    <Self as diesel::associations::HasTable>::Table,
  >
  where
    Self: diesel::associations::HasTable;

  async fn del_by(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> IoResult<()>
  where
    Self: Sized + diesel::associations::HasTable,
    <Self as diesel::associations::HasTable>::Table: diesel::query_builder::QueryId + 'static,
    <<Self as diesel::associations::HasTable>::Table as diesel::QuerySource>::FromClause: diesel::query_builder::QueryFragment<diesel::pg::Pg>,
  {
    log::trace!("{}::delete_by: {filter:?}", Self::get_name());
    let pool = pool.clone();
    let filter = filter.clone();
    ntex::rt::spawn_blocking(move || {
      let query = Self::gen_del_query(&filter);
      let mut conn = utils::store::get_pool_conn(&pool)?;
      query.execute(&mut conn).map_err(Self::map_err)?;
      Ok(())
    })
    .await?
  }
}
