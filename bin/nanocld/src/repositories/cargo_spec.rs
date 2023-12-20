use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  cargo_spec::CargoSpec,
};

use crate::{
  gen_where4json, gen_where4string,
  models::{Pool, CargoSpecDb, FromSpec},
  schema::cargo_specs,
};

use super::generic::*;

impl RepositoryBase for CargoSpecDb {}

impl RepositoryCreate for CargoSpecDb {}

impl RepositoryDelByPk for CargoSpecDb {}

impl RepositoryRead for CargoSpecDb {
  type Output = CargoSpecDb;
  type Query = cargo_specs::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = cargo_specs::table.into_boxed();
    if let Some(value) = r#where.get("cargo_key") {
      gen_where4string!(query, cargo_specs::cargo_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, cargo_specs::version, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, cargo_specs::data, value);
    }
    if is_multiple {
      query = query.order(cargo_specs::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl RepositoryDelBy for CargoSpecDb {
  fn gen_del_query(
    filter: &GenericFilter,
  ) -> diesel::query_builder::BoxedDeleteStatement<
    'static,
    diesel::pg::Pg,
    <Self as diesel::associations::HasTable>::Table,
  >
  where
    Self: diesel::associations::HasTable,
  {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = diesel::delete(cargo_specs::table).into_boxed();
    if let Some(value) = r#where.get("cargo_key") {
      gen_where4string!(query, cargo_specs::cargo_key, value);
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, cargo_specs::version, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, cargo_specs::data, value);
    }
    query
  }
}

impl CargoSpecDb {
  pub(crate) async fn find_by_cargo(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<CargoSpec>> {
    let filter = GenericFilter::new()
      .r#where("cargo_key", GenericClause::Eq(name.to_owned()));
    let items = CargoSpecDb::read(&filter, pool)
      .await??
      .into_iter()
      .map(|item| item.try_to_spec())
      .collect::<IoResult<Vec<_>>>()?;
    Ok(items)
  }
}
