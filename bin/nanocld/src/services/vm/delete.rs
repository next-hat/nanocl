use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SystemState, VmDb},
  utils,
};

/// Delete a virtual machine by canonical key
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Vms",
  path = "/vms/{key}",
  params(
    ("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 200, description = "Virtual machine deletion accepted", headers(
      ("X-Nanocl-Operation" = String, description = "Accepted operation UUID. Match terminal event Metadata.Operation to confirm completion.")
    )),
    (status = 404, description = "The virtual machine does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn delete_vm(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let (_, operation) = VmDb::delete_with_receipt(key.as_str(), state).await?;
  Ok(
    web::HttpResponse::Ok()
      .set_header("X-Nanocl-Operation", operation.to_string())
      .build(),
  )
}
