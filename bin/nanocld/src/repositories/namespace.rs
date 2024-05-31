use std::collections::HashMap;

use diesel::prelude::*;

use bollard_next::network::InspectNetworkOptions;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{generic::GenericFilter, namespace::NamespaceSummary};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  schema::namespaces,
  models::{CargoDb, ColumnType, NamespaceDb, ProcessDb, SystemState},
};

use super::generic::*;

impl RepositoryBase for NamespaceDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("name", (ColumnType::Text, "namespaces.name")),
      (
        "created_at",
        (ColumnType::Timestamptz, "namespaces.created_at"),
      ),
    ])
  }
}

impl RepositoryCreate for NamespaceDb {}

impl RepositoryDelByPk for NamespaceDb {}

impl RepositoryReadBy for NamespaceDb {
  type Output = NamespaceDb;

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
    let mut query = namespaces::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(namespaces::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for NamespaceDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, i64> {
    let mut query = namespaces::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl NamespaceDb {
  /// List all existing namespaces
  pub async fn list(
    filter: &GenericFilter,
    state: &SystemState,
  ) -> HttpResult<Vec<NamespaceSummary>> {
    let items = NamespaceDb::read_by(filter, &state.inner.pool).await?;
    let mut new_items = Vec::new();
    for item in items {
      let cargo_count =
        CargoDb::count_by_namespace(&item.name, &state.inner.pool).await?;
      let processes =
        ProcessDb::list_by_namespace(&item.name, &state.inner.pool).await?;
      let network = state
        .inner
        .docker_api
        .inspect_network(&item.name, None::<InspectNetworkOptions<String>>)
        .await?;
      let ipam = network.ipam.unwrap_or_default();
      let ipam_config = ipam.config.unwrap_or_default();
      let gateway = ipam_config
        .first()
        .ok_or(HttpError::internal_server_error(format!(
          "Unable to get gateway for network {}",
          &item.name
        )))?
        .gateway
        .clone()
        .unwrap_or_default();
      new_items.push(NamespaceSummary {
        name: item.name.to_owned(),
        cargoes: cargo_count as usize,
        instances: processes.len(),
        gateway,
        created_at: item.created_at,
      })
    }
    Ok(new_items)
  }
}
