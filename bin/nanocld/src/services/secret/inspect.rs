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
    (status = 200, description = "Detailed information about a secret", body = Secret),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::get("/secrets/{key}/inspect")]
pub async fn inspect_secret(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let secret =
    SecretDb::transform_read_by_pk(&path.1, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&secret))
}
