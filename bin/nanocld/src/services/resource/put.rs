use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::resource::{ResourcePartial, ResourceUpdate};

use crate::{
  models::{ResourceDb, SystemState},
  objects::generic::*,
  repositories::generic::*,
};

/// Create a new resource spec and add history entry
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  request_body = ResourceUpdate,
  tag = "Resources",
  path = "/resources/{name}",
  params(
    ("name" = String, Path, description = "Name of the resource")
  ),
  responses(
    (status = 200, description = "Resource updated", body = Resource),
    (status = 404, description = "Resource does not exit", body = ApiError),
  ),
))]
#[web::put("/resources/{name}")]
pub async fn put_resource(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceUpdate>,
) -> HttpResult<web::HttpResponse> {
  let resource =
    ResourceDb::transform_read_by_pk(&path.1, &state.inner.pool).await?;
  let new_resource = ResourcePartial {
    name: path.1.clone(),
    kind: resource.kind,
    data: payload.data.clone(),
    metadata: payload.metadata.clone(),
  };
  let resource =
    ResourceDb::put_obj_by_pk(&path.1, &new_resource, &state).await?;
  Ok(web::HttpResponse::Ok().json(&resource))
}
