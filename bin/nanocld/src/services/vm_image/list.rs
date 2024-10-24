use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{SystemState, VmImageDb},
  repositories::generic::*,
  utils,
};

/// List virtual machine images with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "VmImages",
  path = "/vms/images",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"my-image\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of vm images", body = [VmImage]),
  ),
))]
#[web::get("/vms/images")]
pub async fn list_vm_images(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let images = VmImageDb::read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&images))
}
