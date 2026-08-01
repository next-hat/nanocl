use ntex::web;

use nanocl_error::{http::HttpResult, io::IoResult};

use crate::{
  models::{SpecDb, SystemState},
  utils,
};

/// List virtual machine histories
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{key}/histories",
  params(
    ("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 200, description = "The virtual machine histories have been listed", body = [nanocl_stubs::vm_spec::VmSpec]),
    (status = 404, description = "The virtual machine does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::get("/vms/{key}/histories")]
pub async fn list_vm_history(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let histories = SpecDb::read_by_kind_key(key.as_str(), &state.inner.pool)
    .await?
    .into_iter()
    .map(|i| i.try_to_vm_spec())
    .collect::<IoResult<Vec<_>>>()?;
  Ok(web::HttpResponse::Ok().json(&histories))
}
