use diesel::prelude::*;

use bollard_next::network::InspectNetworkOptions;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{generic::GenericFilter, namespace::NamespaceSummary};

use crate::{
  schema::namespaces,
  gen_multiple, gen_where4string,
  models::{CargoDb, NamespaceDb, ProcessDb, SystemState},
};

use super::generic::*;

impl RepositoryBase for NamespaceDb {}

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
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = namespaces::table.into_boxed();
    if let Some(name) = r#where.get("name") {
      gen_where4string!(query, namespaces::name, name);
    }
    if is_multiple {
      gen_multiple!(query, namespaces::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for NamespaceDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, i64> {
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = namespaces::table.into_boxed();
    if let Some(name) = r#where.get("name") {
      gen_where4string!(query, namespaces::name, name);
    }
    query.count()
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
