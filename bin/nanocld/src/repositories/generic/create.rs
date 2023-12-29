use std::sync::Arc;

use diesel::{prelude::*, associations::HasTable};

use nanocl_error::io::IoResult;

use crate::{utils, models::Pool};

pub trait RepositoryCreate: super::RepositoryBase {
  async fn create_from<I>(item: I, pool: &Pool) -> IoResult<Self>
  where
    Self: Sized
      + Send
      + From<I>
      + HasTable
      + diesel::Insertable<Self::Table>
      + 'static,
    Self::Table: HasTable<Table = Self::Table> + diesel::Table,
    diesel::query_builder::InsertStatement<
      Self::Table,
      <Self as diesel::Insertable<Self::Table>>::Values,
    >: diesel::query_dsl::LoadQuery<'static, diesel::pg::PgConnection, Self>,
  {
    let pool = Arc::clone(pool);
    let item = Self::from(item);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = diesel::insert_into(<Self::Table as HasTable>::table())
        .values(item)
        .get_result(&mut conn)
        .map_err(Self::map_err)?;
      Ok(item)
    })
    .await?
  }
}
