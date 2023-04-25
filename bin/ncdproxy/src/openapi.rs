use std::sync::Arc;

use ntex::web;
use ntex::http;
use ntex::util::Bytes;
use utoipa::OpenApi;
use utoipa_swagger_ui::Config;

use nanocl_utils::http_error::HttpError;
use nanocld_client::stubs::proxy::{
  ProxyRule, ProxyRuleHttp, ProxyRuleStream, ResourceProxyRule,
  ProxyHttpLocation, ProxySslConfig, ProxyStreamProtocol, StreamTarget,
  LocationTarget, CargoTarget, HttpTarget, UriTarget, UrlRedirect,
};

use crate::service;

/// Main structure to generate OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
  paths(
    service::apply_rule,
    service::remove_rule,
  ),
  components(schemas(
    ResourceProxyRule,
    ProxyRule,
    ProxyRuleHttp,
    ProxyRuleStream,
    ProxyHttpLocation,
    ProxySslConfig,
    ProxyStreamProtocol,
    StreamTarget,
    LocationTarget,
    CargoTarget,
    HttpTarget,
    UriTarget,
    UrlRedirect,
  )),
  tags(
    (name = "Rules", description = "Rules management endpoints."),
  ),
)]
struct ApiDoc;

#[web::get("/explorer/swagger.json")]
async fn get_api_specs() -> Result<web::HttpResponse, HttpError> {
  let spec = ApiDoc::openapi().to_json().map_err(|err| HttpError {
    status: http::StatusCode::INTERNAL_SERVER_ERROR,
    msg: format!("Error generating OpenAPI spec: {}", err),
  })?;
  return Ok(
    web::HttpResponse::Ok()
      .content_type("application/json")
      .body(spec),
  );
}

#[web::get("/explorer{tail}*")]
async fn openapi_explorer(
  tail: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpError> {
  if !tail.starts_with('/') {
    return Ok(
      web::HttpResponse::Found()
        .header(http::header::LOCATION, "/explorer/".to_string())
        .finish(),
    );
  }
  let path = tail.trim_start_matches('/');
  let config =
    Arc::new(Config::new(["/explorer/swagger.json"]).use_base_layout());
  match utoipa_swagger_ui::serve(path, config)
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

pub fn ntex_config(config: &mut web::ServiceConfig) {
  let yaml = ApiDoc::openapi()
    .to_yaml()
    .expect("Unable to generate openapi spec");
  std::fs::write("./bin/ncdproxy/specs/swagger.yaml", yaml).unwrap();
  config.service(get_api_specs);
  config.service(openapi_explorer);
}
