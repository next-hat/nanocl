use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{NamespaceDb, SystemState},
  utils,
};

/// List namespaces with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Namespaces",
  path = "/namespaces",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"test\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of namespace", body = [NamespaceSummary]),
  ),
))]
#[web::get("/namespaces")]
pub async fn list_namespace(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let items = NamespaceDb::list(&filter, &state).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}
