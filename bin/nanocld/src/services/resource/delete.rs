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
    (status = 404, description = "Resource doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn delete_resource(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let state = state.clone();
  let key = path.1.clone();
  ResourceDb::mutate(&key.clone(), async move {
    ResourceDb::del_obj_by_pk(&key, &(), &state).await
  })
  .await?;
  Ok(web::HttpResponse::Accepted().build())
}
