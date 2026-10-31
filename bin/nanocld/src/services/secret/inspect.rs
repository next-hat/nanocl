use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SecretDb, SystemState},
  repositories::generic::*,
};

/// Get detailed information about a secret
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Secrets",
  path = "/secrets/{key}/inspect",
  params(
    ("key" = String, Path, description = "Key of the secret")
  ),
  responses(
    (status = 200, description = "Detailed information about a secret", body = nanocl_stubs::secret::Secret),
    (status = 404, description = "Secret doesn't", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_secret(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let secret =
    SecretDb::transform_read_by_pk(&path.1, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&secret))
}
