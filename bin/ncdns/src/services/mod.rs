use ntex::web;

use nanocl_error::http::HttpError;

use nanocl_utils::ntex::middlewares;

use crate::vars;

#[cfg(feature = "dev")]
mod openapi;

mod rule;

pub async fn unhandled() -> Result<web::HttpResponse, HttpError> {
  Err(HttpError::not_found("Route or method unhandled"))
}

/// Health check endpoint to verify that the service is running and responsive.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Health",
  path = "/health",
  responses(
    (status = 200, description = "The health status", body = String),
  ),
))]
pub async fn health() -> Result<web::HttpResponse, HttpError> {
  Ok(web::HttpResponse::Ok().json(&serde_json::json!({
    "status": "ok",
    "version": vars::VERSION,
    "commit_id": vars::COMMIT_ID,
    "channel": vars::CHANNEL,
  })))
}

pub fn app_config() -> web::WebAppConfig {
  #[allow(unused_mut)]
  let mut config = web::WebAppConfig::new();
  #[cfg(feature = "dev")]
  {
    use nanocl_utils::ntex::swagger;
    use openapi::ApiDoc;
    use utoipa::OpenApi;

    let api_doc = ApiDoc::openapi();
    std::fs::write(
      "./bin/ncdns/specs/swagger.yaml",
      api_doc.to_yaml().expect("Unable to convert ApiDoc to yaml"),
    )
    .expect("Unable to write swagger.yaml");
    let swagger_conf =
      swagger::SwaggerConfig::new(api_doc, "/explorer/swagger.json");
    config = config.set_state(swagger_conf);
  }
  config
}

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  #[cfg(feature = "dev")]
  config.service(
    web::scope("/explorer/").configure(nanocl_utils::ntex::swagger::register),
  );
  config.service(
    web::resource("/health")
      .name("health")
      .guard(web::guard::Get())
      .to(health),
  );
  let versioning = middlewares::Versioning::new(vars::VERSION).finish();
  config.service(
    web::scope("/{version}")
      .middleware(versioning)
      .configure(rule::ntex_config),
  );
}
