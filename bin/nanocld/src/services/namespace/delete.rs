use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use crate::{
  models::{NamespaceDb, SystemState},
  objects::generic::*,
};

/// Delete a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Namespaces",
  path = "/namespaces/{name}",
  params(
    ("name" = String, Path, description = "Name of the namespace to delete")
  ),
  responses(
    (status = 202, description = "Namespace have been deleted"),
    (status = 404, description = "Namespace is not existing", body = crate::services::openapi::ApiError),
    (status = 403, description = "Deletion of this namespace is forbidden", body = crate::services::openapi::ApiError),
  ),
))]
#[web::delete("/namespaces/{name}")]
pub async fn delete_namespace(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  if path.1 == "global" || path.1 == "system" {
    return Err(HttpError::forbidden(
      "Deletion of this namespace is forbidden",
    ));
  }
  NamespaceDb::del_obj_by_pk(&path.1, &(), &state).await?;
  Ok(web::HttpResponse::Accepted().into())
}
