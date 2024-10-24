use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{ResourceDb, SystemState},
  objects::generic::*,
};

/// Delete a resource by name
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Resources",
  path = "/resources/{name}",
  params(
    ("name" = String, Path, description = "The resource name to delete")
  ),
  responses(
    (status = 202, description = "The resource and his history has been deleted"),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::delete("/resources/{name}")]
pub async fn delete_resource(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  ResourceDb::del_obj_by_pk(&path.1, &(), &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}
