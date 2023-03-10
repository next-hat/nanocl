use ntex::web;
use utoipa::OpenApi;
use nanocl_stubs::cargo::*;
use nanocl_stubs::cargo_config::*;
use crate::models::*;

use nanocl_stubs::namespace::*;

use nanocl_stubs::cargo_image::*;

use crate::services::*;

#[derive(OpenApi)]
#[openapi(
  paths(
    // Namespace
    namespace::list_namespace,
    // namespace::create_namespace,
    // namespace::delete_namespace_by_name,
    // namespace::inspect_namespace_by_name,

    // // proxy template
    // proxy_template::list_proxy_template,

    // // Cargo images
    // cargo_image::list_cargo_image,
    // cargo_image::create_cargo_image,
    // cargo_image::inspect_cargo_image,
    // cargo_image::delete_cargo_image_by_name,

    // // Cargo
    // cargo::create_cargo,
    // cargo::list_cargo,
    // cargo::delete_cargo,
    // cargo::start_cargo,
    // cargo::stop_cargo,
    // cargo::patch_cargo,
  ),
  components(
    schemas(NamespaceSummary),
    // schemas(ApiError),
    // schemas(GenericDelete),

    // // Proxy template
    // schemas(ProxyTemplateItem),
    // schemas(ProxyTemplateModes),

    // // Namespace
    // schemas(NamespaceItem),
    // schemas(NamespacePartial),

    // // Cargo images
    // schemas(CargoImagePartial),

    // // Cargo
    // schemas(Cargo),
    // schemas(CargoPartial),
    // schemas(CargoConfig),
    // schemas(CargoConfigPartial),
    // schemas(CargoSummary),
    // schemas(CargoReplication),
    // schemas(ReplicaValue),

    // Todo Docker network struct bindings
    // Network,
    // Ipam,
    // IpamConfig,
    // NetworkContainer,
  )
)]
pub struct ApiDoc;

pub fn to_json() -> String {
  ApiDoc::openapi().to_pretty_json().unwrap()
}

#[web::get("/explorer/swagger.json")]
async fn get_api_specs() -> Result<web::HttpResponse, web::Error> {
  let api_spec = to_json();
  return Ok(
    web::HttpResponse::Ok()
      .header("Access-Control-Allow", "*")
      .content_type("application/json")
      .body(api_spec),
  );
}

#[cfg(feature = "dev")]
pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(get_api_specs);
  // config.service(
  //   // static files
  //   fs::Files::new("/explorer", "./swagger-ui/").index_file("index.html"),
  // );
}
