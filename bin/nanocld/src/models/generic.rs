use std::sync::Arc;

use tokio::task::JoinHandle;
use diesel::query_dsl::methods::{FindDsl, FilterDsl};
use diesel::{associations, RunQueryDsl, query_builder, pg, query_dsl};

use nanocl_error::io::{IoResult, FromIo};
use nanocl_stubs::generic::GenericFilter;

use crate::utils;

use super::Pool;

/// Generic trait to convert a metric type into a insertable database type
pub trait ToMeticDb {
  type MetricDb;

  fn to_metric_db(self, node_name: &str) -> Self::MetricDb;
}

/// Generic trait to convert a spec type into a insertable database type and vise versa
pub trait FromSpec {
  type Spec;
  type SpecPartial;

  fn try_to_data(p: &Self::SpecPartial) -> IoResult<serde_json::Value>
  where
    Self::SpecPartial: serde::Serialize,
  {
    let mut data =
      serde_json::to_value(p).map_err(|err| err.map_err_context(|| "Spec"))?;
    if let Some(meta) = data.as_object_mut() {
      meta.remove("Metadata");
    }
    Ok(data)
  }

  fn get_data(&self) -> &serde_json::Value;

  fn to_spec(&self, p: &Self::SpecPartial) -> Self::Spec;

  fn try_from_spec_partial(
    id: &str,
    version: &str,
    p: &Self::SpecPartial,
  ) -> IoResult<Self>
  where
    Self: std::marker::Sized;

  fn try_to_spec(&self) -> IoResult<Self::Spec>
  where
    Self::SpecPartial: serde::de::DeserializeOwned,
    Self::Spec: std::marker::Sized,
  {
    let p =
      serde_json::from_value::<Self::SpecPartial>(self.get_data().clone())
        .map_err(|err| err.map_err_context(|| "Spec"))?;
    Ok(self.to_spec(&p))
  }
}

/// Trait to add relation with a spec
pub trait WithSpec {
  type Type;
  type Relation;

  fn with_spec(self, s: &Self::Relation) -> Self::Type;
}

pub trait Repository {
  type Table;
  type Item;
  type UpdateItem;

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>>;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>>;

  fn create<I>(item: I, pool: &Pool) -> JoinHandle<IoResult<Self>>
  where
    Self: From<I>,
    Self: diesel::Insertable<Self::Table>,
    Self: std::marker::Sized + Send + 'static,
    Self::Table: associations::HasTable<Table = Self::Table> + diesel::Table,
    query_builder::InsertStatement<
      Self::Table,
      <Self as diesel::Insertable<Self::Table>>::Values,
    >: query_dsl::LoadQuery<'static, pg::PgConnection, Self> + Send,
  {
    let pool = Arc::clone(pool);
    let item = Self::from(item);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item =
        diesel::insert_into(<Self::Table as associations::HasTable>::table())
          .values(item)
          .get_result(&mut conn)
          .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      Ok(item)
    })
  }

  fn delete_by_pk<Pk>(
    pk: &Pk,
    pool: &Pool,
  ) -> JoinHandle<IoResult<()>>
  where
    Pk: ToOwned + ?Sized,
    <Pk as ToOwned>::Owned: Send + 'static,
    Self::Table: query_dsl::methods::FindDsl<<Pk as ToOwned>::Owned> + associations::HasTable<Table = Self::Table>,
    diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned>: query_builder::IntoUpdateTarget,
    query_builder::DeleteStatement<
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as associations::HasTable>::Table,
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as query_builder::IntoUpdateTarget>::WhereClause,
    >: query_builder::QueryFragment<pg::Pg> + query_builder::QueryId,
  {
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      diesel::delete(<Self::Table as associations::HasTable>::table().find(pk))
        .execute(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      Ok(())
    })
  }

  fn find_by_pk<Pk>(pk: &Pk, pool: &Pool) -> JoinHandle<IoResult<Self>>
  where
    // R: TryFrom<Self> + Send + 'static,
    Pk: ToOwned + ?Sized,
    <Pk as ToOwned>::Owned: Send + 'static,
    Self: std::marker::Sized + Send + 'static,
    Self::Table: query_dsl::methods::FindDsl<<Pk as ToOwned>::Owned>
      + associations::HasTable<Table = Self::Table>,
    diesel::dsl::Find<Self::Table, <Pk as ToOwned>::Owned>:
      diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, Self>,
  {
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let query = <Self::Table as associations::HasTable>::table().find(pk);
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let res = query
        .get_result::<Self>(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      // let res = R::try_from(res).map_err(|_| {
      //   IoError::invalid_data(std::any::type_name::<Self>(), "try_from")
      // })?;
      Ok(res)
    })
  }

  fn update_by_pk<T, Pk>(
    pk: &Pk,
    values: T,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self>>
  where
    T: Into<Self::UpdateItem>,
    Pk: ToOwned + ?Sized,
    <Pk as ToOwned>::Owned: Send + 'static,
    Self: std::marker::Sized + Send + 'static,
    Self::Table: diesel::query_dsl::methods::FindDsl<<Pk as ToOwned>::Owned> + associations::HasTable<Table = Self::Table>,
    Self::UpdateItem: diesel::AsChangeset<
        Target = <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as associations::HasTable>::Table,
      > + Send
      + 'static,
    diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned>: query_builder::IntoUpdateTarget,
    diesel::query_builder::UpdateStatement<
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as associations::HasTable>::Table,
      <diesel::helper_types::Find<Self::Table, <Pk as ToOwned>::Owned> as query_builder::IntoUpdateTarget>::WhereClause,
      <Self::UpdateItem as diesel::AsChangeset>::Changeset,
    >:
      diesel::query_builder::AsQuery + diesel::query_dsl::LoadQuery<'static, pg::PgConnection, Self>,
  {
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    let values = values.into();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let res = diesel::update(
        <Self::Table as associations::HasTable>::table().find(pk),
      )
      .set(values)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      Ok(res)
    })
  }

  fn delete_by<P>(
    predicate: P,
    pool: &Pool,
  ) -> JoinHandle<IoResult<()>>
  where
    Self::Table: query_dsl::methods::FilterDsl<P> + associations::HasTable<Table = Self::Table>,
    diesel::helper_types::Filter<Self::Table, P>: query_builder::IntoUpdateTarget,
    query_builder::DeleteStatement<
      <diesel::helper_types::Filter<Self::Table, P> as associations::HasTable>::Table,
      <diesel::helper_types::Filter<Self::Table, P> as query_builder::IntoUpdateTarget>::WhereClause,
    >: query_builder::QueryFragment<pg::Pg> + query_builder::QueryId,
    P: Send + 'static,
  {
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      diesel::delete(
        <Self::Table as associations::HasTable>::table().filter(predicate),
      )
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?;
      Ok(())
    })
  }
}
