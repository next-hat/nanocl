use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericClause, GenericCount, GenericListQueryNsp};

use crate::{
  models::{SystemState, VmDb},
  repositories::generic::*,
  utils,
};

/// Count vm images
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"global\" } } } }"),
  ),
  responses(
    (status = 200, description = "Count result", body = GenericCount),
  ),
))]
#[web::get("/vms/count")]
pub async fn count_vm(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQueryNsp>,
) -> HttpResult<web::HttpResponse> {
  let query = utils::query_string::parse_qs_nsp_filter(&qs)?;
  let namespace = utils::key::resolve_nsp(&query.namespace);
  let filter = query
    .filter
    .unwrap_or_default()
    .r#where("namespace_name", GenericClause::Eq(namespace));
  let count = VmDb::count_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
