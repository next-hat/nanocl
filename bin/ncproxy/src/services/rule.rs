use ntex::web;

use nanocl_error::http::HttpError;

use nanocld_client::stubs::proxy::ResourceProxyRule;

use crate::{utils, models::SystemStateRef};

/// Create/Update a new ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Rules",
  path = "/rules/{name}",
  request_body = ResourceProxyRule,
  params(
    ("name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "The created rule", body = ResourceProxyRule),
  ),
))]
#[web::put("/rules/{name}")]
pub async fn apply_rule(
  state: web::types::State<SystemStateRef>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceProxyRule>,
) -> Result<web::HttpResponse, HttpError> {
  log::info!("apply_rule: {}", path.1);
  utils::nginx::add_rule(&path.1, &payload, &state).await?;
  state.event_emitter.emit_reload().await;
  Ok(web::HttpResponse::Ok().json(&payload.into_inner()))
}

/// Delete a ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Rules",
  path = "/rules/{name}",
  params(
    ("name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "Rule has been deleted"),
  ),
))]
#[web::delete("/rules/{name}")]
pub async fn remove_rule(
  state: web::types::State<SystemStateRef>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpError> {
  log::info!("remove_rule: {}", path.1);
  utils::nginx::del_rule(&path.1, &state).await?;
  state.event_emitter.emit_reload().await;
  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(apply_rule);
  config.service(remove_rule);
}
