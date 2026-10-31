use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::namespace::NamespacePartial;

use crate::{
  models::{NamespaceDb, SystemState},
  objects::generic::*,
};

/// Create a new namespace
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = NamespacePartial,
  tag = "Namespaces",
  path = "/namespaces",
  responses(
    (status = 200, description = "The created namespace", body = nanocl_stubs::namespace::Namespace),
    (status = 409, description = "Namespace already exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn create_namespace(
  state: &SystemState,
  _request_state: (),
  payload: web::types::Json<NamespacePartial>,
) -> HttpResult<web::HttpResponse> {
  let item = NamespaceDb::create_obj(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&item))
}
