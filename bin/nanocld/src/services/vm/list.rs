use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQueryNsp;

use crate::{
  models::{SystemState, VmDb},
  utils,
};

/// List virtual machines with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"my-vm\" } } } }"),
    ("namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "List of virtual machine", body = [VmSummary]),
  ),
))]
#[web::get("/vms")]
pub async fn list_vm(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQueryNsp>,
) -> HttpResult<web::HttpResponse> {
  let query = utils::query_string::parse_qs_nsp_filter(&qs)?;
  let vms = VmDb::list(&query, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&vms))
}
