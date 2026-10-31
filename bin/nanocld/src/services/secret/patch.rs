use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::secret::SecretUpdate;

use crate::{
  models::{SecretDb, SystemState},
  objects::generic::*,
};

/// Update a secret
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Secrets",
  request_body = SecretUpdate,
  path = "/secrets/{key}",
  params(
    ("key" = String, Path, description = "Key of the secret"),
  ),
  responses(
    (status = 200, description = "Secret patched", body = nanocl_stubs::secret::Secret),
    (status = 404, description = "Secret does't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn patch_secret(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<SecretUpdate>,
) -> HttpResult<web::HttpResponse> {
  let item = SecretDb::patch_obj_by_pk(&path.1, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().json(&item))
}
