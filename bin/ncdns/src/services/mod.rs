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
#[web::get("/health")]
pub async fn health() -> Result<web::HttpResponse, HttpError> {
  Ok(web::HttpResponse::Ok().json(&serde_json::json!({
    "status": "ok",
    "version": vars::VERSION,
    "commit_id": vars::COMMIT_ID,
    "channel": vars::CHANNEL,
  })))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
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
    config.service(
      web::scope("/explorer/")
        .state(swagger_conf)
        .configure(swagger::register),
    );
  }
  config.service(health);
  let versioning = middlewares::Versioning::new(vars::VERSION).finish();
  config.service(
    web::scope("/{version}")
      .middleware(versioning)
      .configure(rule::ntex_config),
  );
}
