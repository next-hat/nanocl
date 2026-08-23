use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericCount, GenericListQueryNsp};

use crate::{
  models::{SystemState, VmDb},
  utils,
};

/// Count vm images
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"global\" } } } }"),
    ("namespace" = Option<String>, Query, description = "Optional namespace filter; omitted or blank returns all namespaces"),
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
  let count = VmDb::count(&query, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
