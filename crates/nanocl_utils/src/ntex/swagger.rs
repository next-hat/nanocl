use std::sync::Arc;

use ntex::{util::Bytes, web};

use nanocl_error::http::{HttpError, HttpResult};

pub struct SwaggerConfig {
  config: Arc<utoipa_swagger_ui::Config<'static>>,
  spec: Box<utoipa::openapi::OpenApi>,
}

impl SwaggerConfig {
  pub fn new(
    spec: Box<utoipa::openapi::OpenApi>,
    def_url: &'static str,
  ) -> Self {
    Self {
      config: Arc::new(
        utoipa_swagger_ui::Config::new([def_url]).use_base_layout(),
      ),
      spec,
    }
  }

  pub fn config(
    &mut self,
    config: utoipa_swagger_ui::Config<'static>,
  ) -> &mut Self {
    self.config = Arc::new(config);
    self
  }
}

#[web::get("/swagger.json")]
async fn get_specs(
  openapi_conf: web::types::State<SwaggerConfig>,
) -> HttpResult<web::HttpResponse> {
  let spec = openapi_conf.spec.to_json().map_err(|err| {
    HttpError::internal_server_error(format!(
      "Failed to serialize OpenAPI: {}",
      err
    ))
  })?;
  return Ok(
    web::HttpResponse::Ok()
      .content_type("application/json")
      .body(spec),
  );
}

#[web::get("/{tail}*")]
async fn get_swagger(
  tail: web::types::Path<String>,
  openapi_conf: web::types::State<SwaggerConfig>,
) -> HttpResult<web::HttpResponse> {
  match utoipa_swagger_ui::serve(&tail, openapi_conf.config.clone())
    .map_err(|err| HttpError::internal_server_error(err.to_string()))?
  {
    None => Err(HttpError::not_found("Path not handled")),
    Some(file) => Ok({
      let bytes = Bytes::from(file.bytes.to_vec());
      web::HttpResponse::Ok()
        .content_type(file.content_type)
        .body(bytes)
    }),
  }
}

pub fn register(config: &mut web::ServiceConfig) {
  config.service(get_specs);
  config.service(get_swagger);
}
