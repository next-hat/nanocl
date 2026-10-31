use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

#[cfg(feature = "dev")]
pub mod openapi;

mod cargo;
mod event;
mod job;
mod metric;
mod namespace;
mod network;
mod node;
mod process;
mod resource;
mod resource_kind;
mod secret;
mod system;
pub(crate) mod vm;

pub async fn unhandled() -> HttpResult<web::HttpResponse> {
  Err(HttpError::not_found("Route or method unhandled"))
}

pub fn app_config() -> web::WebAppConfig {
  #[allow(unused_mut)]
  let mut config = web::WebAppConfig::new()
    .set_state(web::types::PayloadConfig::new(20_000_000_000))
    .set_state(web::types::JsonConfig::default().limit(20_000_000_000));
  #[cfg(feature = "dev")]
  {
    use nanocl_utils::ntex::swagger;
    use openapi::ApiDoc;
    use utoipa::OpenApi;
    let api_doc = ApiDoc::openapi();
    std::fs::write(
      "./bin/nanocld/specs/swagger.yaml",
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
    web::scope("/{version}")
      .middleware(
        nanocl_utils::ntex::middlewares::Versioning::new(crate::vars::VERSION)
          .finish(),
      )
      .configure(|config| {
        network::ntex_config(config);
        node::ntex_config(config);
        namespace::ntex_config(config);
        system::ntex_config(config);
        resource::ntex_config(config);
        cargo::ntex_config(config);
        vm::ntex_config(config);
        metric::ntex_config(config);
        secret::ntex_config(config);
        process::ntex_config(config);
        job::ntex_config(config);
        event::ntex_config(config);
        resource_kind::ntex_config(config);
      }),
  );
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use nanocl_stubs::system::BinaryInfo;

  use super::ntex_config;

  use crate::{utils::tests::*, vars};

  #[ntex::test]
  pub async fn get_version() {
    let client = gen_test_system(ntex_config, vars::VERSION).await.client;
    let res = client.send_get("/version", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "version");
    let data = res.json::<BinaryInfo>().await.unwrap();
    assert_eq!(data.arch, vars::ARCH, "Expect arch to be {}", vars::ARCH);
    assert_eq!(
      data.version,
      vars::VERSION,
      "Expect version to be {}",
      vars::VERSION
    );
    assert_eq!(
      data.commit_id,
      vars::COMMIT_ID,
      "Expect commit_id to be {}",
      vars::COMMIT_ID
    );
  }

  #[ntex::test]
  async fn ping() {
    let client = gen_test_system(ntex_config, vars::VERSION).await.client;
    let res = client.send_head("/_ping", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::ACCEPTED, "ping");
  }

  #[ntex::test]
  async fn unhandled_route() {
    let client = gen_test_system(ntex_config, vars::VERSION).await.client;
    let res = client.send_get("/v0.1/unhandled", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::NOT_FOUND, "unhandled");
  }

  #[ntex::test]
  async fn test_wrong_version() {
    let client = gen_test_system(ntex_config, "0.9999").await.client;
    let res = client.send_get("/version", None::<String>).await;
    assert_eq!(res.status(), http::StatusCode::NOT_FOUND);
    let version = res.headers().get("x-api-version");
    assert!(version.is_some());
    assert_eq!(version.unwrap(), vars::VERSION);
    let client = gen_test_system(ntex_config, "xdlol").await.client;
    let res = client.send_get("/version", None::<String>).await;
    assert_eq!(res.status(), http::StatusCode::NOT_FOUND);
    let version = res.headers().get("x-api-version");
    assert!(version.is_some());
    assert_eq!(version.unwrap(), vars::VERSION);
  }
}
