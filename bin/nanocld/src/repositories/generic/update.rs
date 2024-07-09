use diesel::{associations, prelude::*};

use nanocl_error::io::IoResult;

use crate::{models::Pool, utils};

pub trait RepositoryUpdate: super::RepositoryBase {
  type UpdateItem;

  async fn update_pk<T, Pk>(
    pk: &Pk,
    values: T,
    pool: &Pool,
  ) -> IoResult<Self>
  where
    T: Into<Self::UpdateItem>,
    Pk: ToOwned + ?Sized + std::fmt::Display,
    <Pk as ToOwned>::Owned: Send + 'static,
    Self: Sized + Send + associations::HasTable + 'static,
    <Self as associations::HasTable>::Table: diesel::query_dsl::methods::FindDsl<<Pk as ToOwned>::Owned> + associations::HasTable<Table = Self::Table>,
    Self::UpdateItem: diesel::AsChangeset<
        Target = <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as associations::HasTable>::Table,
      > + Send
      + 'static,
    diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned>: diesel::query_builder::IntoUpdateTarget,
    diesel::query_builder::UpdateStatement<
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as associations::HasTable>::Table,
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as diesel::query_builder::IntoUpdateTarget>::WhereClause,
      <Self::UpdateItem as diesel::AsChangeset>::Changeset,
    >:
      diesel::query_builder::AsQuery + diesel::query_dsl::LoadQuery<'static, diesel::pg::PgConnection, Self>,
  {
    log::trace!("{}::update_by_pk: {pk}", Self::get_name());
    let pool = pool.clone();
    let pk = pk.to_owned();
    let values = values.into();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = diesel::update(
        <Self::Table as associations::HasTable>::table().find(pk),
      )
      .set(values)
      .get_result(&mut conn)
      .map_err(Self::map_err)?;
      Ok(item)
    })
    .await?
  }
}
