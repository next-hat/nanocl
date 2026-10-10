use std::sync::Arc;

use ntex::{util::Bytes, web};

use nanocl_error::http::{HttpError, HttpResult};

pub struct SwaggerConfig<'a> {
  definition: utoipa::openapi::OpenApi,
  config: Arc<utoipa_swagger_ui::Config<'a>>,
}

impl<'a> SwaggerConfig<'a> {
  pub fn new(definition: utoipa::openapi::OpenApi, def_url: &'a str) -> Self {
    Self {
      definition,
      config: Arc::new(
        utoipa_swagger_ui::Config::new([def_url]).use_base_layout(),
      ),
    }
  }

  pub fn config(&mut self, config: utoipa_swagger_ui::Config<'a>) -> &mut Self {
    self.config = Arc::new(config);
    self
  }
}

async fn get_specs(req: web::HttpRequest) -> HttpResult<web::HttpResponse> {
  let openapi_conf =
    req.app_state::<SwaggerConfig<'static>>().ok_or_else(|| {
      HttpError::internal_server_error("Swagger is not configured")
    })?;
  let spec = openapi_conf.definition.to_json().map_err(|err| {
    HttpError::internal_server_error(format!(
      "Error generating OpenAPI spec: {err}",
    ))
  })?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/json")
      .body(spec),
  )
}

async fn get_swagger(
  tail: web::types::Path<String>,
  req: web::HttpRequest,
) -> HttpResult<web::HttpResponse> {
  let openapi_conf =
    req.app_state::<SwaggerConfig<'static>>().ok_or_else(|| {
      HttpError::internal_server_error("Swagger is not configured")
    })?;
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

pub fn register<St: web::State<Error = web::DefaultError>>(
  config: &mut web::ServiceConfig<St>,
) {
  config.route("/swagger.json", web::get().to(get_specs));
  config.route("/{tail}*", web::get().to(get_swagger));
}
