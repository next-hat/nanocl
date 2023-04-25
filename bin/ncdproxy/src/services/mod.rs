use ntex::web;

use nanocl_utils::ntex::middlewares;
use nanocl_utils::http_error::HttpError;

use crate::version;

#[cfg(feature = "dev")]
mod openapi;

mod rule;
mod system;

pub async fn unhandled() -> Result<web::HttpResponse, HttpError> {
  Err(HttpError {
    status: ntex::http::StatusCode::NOT_FOUND,
    msg: "Route or method unhandled".into(),
  })
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  #[cfg(feature = "dev")]
  {
    use utoipa::OpenApi;
    use nanocl_utils::ntex::swagger;
    use openapi::ApiDoc;

    let swagger_conf =
      swagger::SwaggerConfig::new(ApiDoc::openapi(), "/explorer/swagger.json");
    config.service(
      web::scope("/explorer/")
        .state(swagger_conf)
        .configure(swagger::register),
    );
  }

  let versioning = middlewares::Versioning::new(version::VERSION).finish();

  config.service(
    web::scope("/{version}")
      .wrap(versioning)
      .configure(rule::ntex_config)
      .configure(system::ntex_config),
  );
}
