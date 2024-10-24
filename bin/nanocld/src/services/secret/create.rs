use ntex::web;

use bollard_next::auth::DockerCredentials;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{proxy::ProxySslConfig, secret::SecretPartial};

use crate::{
  models::{SecretDb, SystemState},
  objects::generic::*,
  utils,
};

/// Create a new secret
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = SecretPartial,
  tag = "Secrets",
  path = "/secrets",
  responses(
    (status = 200, description = "List of secret", body = Secret),
    (status = 409, description = "Namespace already exist", body = ApiError),
  ),
))]
#[web::post("/secrets")]
pub async fn create_secret(
  state: web::types::State<SystemState>,
  payload: web::types::Json<SecretPartial>,
) -> HttpResult<web::HttpResponse> {
  utils::key::ensure_kind(&payload.kind)?;
  match payload.kind.as_str() {
    "nanocl.io/tls" => {
      serde_json::from_value::<ProxySslConfig>(payload.data.clone())
        .map_err(|e| HttpError::bad_request(e.to_string()))?;
    }
    "nanocl.io/env" => {
      serde_json::from_value::<Vec<String>>(payload.data.clone())
        .map_err(|e| HttpError::bad_request(e.to_string()))?;
    }
    "nanocl.io/container-registry" => {
      serde_json::from_value::<DockerCredentials>(payload.data.clone())
        .map_err(|e| HttpError::bad_request(e.to_string()))?;
    }
    _ => {}
  }
  let secret = SecretDb::create_obj(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&secret))
}
