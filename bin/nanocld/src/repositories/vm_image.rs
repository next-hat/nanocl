use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::{GenericClause, GenericFilter};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, Pool, VmImageDb, VmImageUpdateDb},
  schema::vm_images,
};

use super::generic::*;

impl RepositoryBase for VmImageDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("name", (ColumnType::Text, "vm_images.name")),
      ("kind", (ColumnType::Text, "vm_images.kind")),
      ("parent", (ColumnType::Text, "vm_images.parent")),
      ("format", (ColumnType::Text, "vm_images.format")),
      ("path", (ColumnType::Text, "vm_images.path")),
      (
        "created_at",
        (ColumnType::Timestamptz, "vm_images.created_at"),
      ),
    ])
  }
}

impl RepositoryCreate for VmImageDb {}

impl RepositoryUpdate for VmImageDb {
  type UpdateItem = VmImageUpdateDb;
}

impl RepositoryDelByPk for VmImageDb {}

impl RepositoryReadBy for VmImageDb {
  type Output = VmImageDb;

  fn get_pk() -> &'static str {
    "name"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::pg::PgConnection,
    Self::Output,
  > {
    let mut query = vm_images::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(vm_images::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for VmImageDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = vm_images::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl VmImageDb {
  pub async fn read_by_parent(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<VmImageDb>> {
    let filter = GenericFilter::new()
      .r#where("parent", GenericClause::Eq(name.to_owned()));
    VmImageDb::read_by(&filter, pool).await
  }
}
