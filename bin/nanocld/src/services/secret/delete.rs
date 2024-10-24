use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SecretDb, SystemState},
  objects::generic::*,
};

/// Delete a secret
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Secrets",
  path = "/secrets/{key}",
  params(
    ("key" = String, Path, description = "Key of the secret")
  ),
  responses(
    (status = 202, description = "Secret have been deleted"),
    (status = 404, description = "Secret don't exists", body = ApiError),
  ),
))]
#[web::delete("/secrets/{key}")]
pub async fn delete_secret(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  SecretDb::del_obj_by_pk(&path.1, &(), &state).await?;
  Ok(web::HttpResponse::Accepted().into())
}
