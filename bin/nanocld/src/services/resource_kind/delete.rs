use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{ResourceKindDb, SpecDb, SystemState},
  repositories::generic::*,
};

/// Delete a resource kind
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "ResourceKinds",
  path = "/resource/kinds/{domain}/{name}",
  params(
    ("domain" = String, Path, description = "Domain of the resource kind"),
    ("name" = String, Path, description = "Name of the resource kind"),
  ),
  responses(
    (status = 202, description = "Resource kind deleted"),
    (status = 404, description = "Resource kind does not exist"),
  ),
))]
#[web::delete("/resource/kinds/{domain}/{name}")]
pub async fn delete_resource_kind(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = format!("{}/{}", path.1, path.2);
  ResourceKindDb::read_by_pk(&key, &state.inner.pool).await?;
  ResourceKindDb::del_by_pk(&key, &state.inner.pool).await?;
  SpecDb::del_by_kind_key(&key, &state.inner.pool).await?;
  Ok(web::HttpResponse::Accepted().into())
}
