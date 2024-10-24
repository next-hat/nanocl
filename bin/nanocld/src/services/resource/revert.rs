use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::resource::ResourcePartial;

use crate::{
  models::{ResourceDb, SpecDb, SystemState},
  objects::generic::*,
  repositories::generic::*,
};

/// Revert a resource to a specific history
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Resources",
  path = "/resources/{name}/histories/{id}/revert",
  params(
    ("name" = String, Path, description = "The resource name to revert"),
    ("id" = String, Path, description = "The resource history id to revert to")
  ),
  responses(
    (status = 200, description = "The resource has been revert", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::patch("/resources/{name}/histories/{id}/revert")]
pub async fn revert_resource(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let history = SpecDb::read_by_pk(&path.2, &state.inner.pool).await?;
  let resource =
    ResourceDb::transform_read_by_pk(&path.1, &state.inner.pool).await?;
  let new_resource = ResourcePartial {
    name: resource.spec.resource_key,
    kind: resource.kind,
    data: history.data,
    metadata: history.metadata,
  };
  let resource =
    ResourceDb::put_obj_by_pk(&path.1, &new_resource, &state).await?;
  Ok(web::HttpResponse::Ok().json(&resource))
}
