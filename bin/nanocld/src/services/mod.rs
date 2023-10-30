use ntex::web;
use ntex::http;
use nanocl_utils::ntex::middlewares;
use nanocl_error::http::HttpError;

use crate::version;

#[cfg(feature = "dev")]
mod openapi;

mod exec;
mod state;
mod node;
mod namespace;
mod system;
mod resource;
mod cargo;
mod cargo_image;
mod metric;
mod http_metric;
mod vm;
mod vm_image;
mod secret;

pub async fn unhandled() -> Result<web::HttpResponse, HttpError> {
  Err(HttpError {
    status: http::StatusCode::NOT_FOUND,
    msg: "Route or method unhandled".into(),
  })
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  #[cfg(feature = "dev")]
  {
    use utoipa::OpenApi;
    use nanocl_utils::ntex::swagger;
    use openapi::ApiDoc;

    let api_doc = ApiDoc::openapi();
    std::fs::write(
      "./bin/nanocld/specs/swagger.yaml",
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

  let versioning = middlewares::Versioning::new(version::VERSION).finish();

  config.service(
    web::scope("/{version}")
      .wrap(versioning)
      .configure(exec::ntex_config)
      .configure(state::ntex_config)
      .configure(node::ntex_config)
      .configure(namespace::ntex_config)
      .configure(system::ntex_config)
      .configure(resource::ntex_config)
      .configure(cargo_image::ntex_config)
      .configure(cargo::ntex_config)
      .configure(vm_image::ntex_config)
      .configure(vm::ntex_config)
      .configure(metric::ntex_config)
      .configure(http_metric::ntex_config)
      .configure(secret::ntex_config),
  );
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use nanocl_stubs::system::Version;

  use super::ntex_config;

  use crate::version;
  use crate::utils::tests::*;

  #[ntex::test]
  pub async fn get_version() {
    let client = gen_test_client(ntex_config, version::VERSION).await;
    let mut res = client.send_get("/version", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "version");
    let data = res.json::<Version>().await.unwrap();
    assert_eq!(
      data.arch,
      version::ARCH,
      "Expect arch to be {}",
      version::ARCH
    );
    assert_eq!(
      data.version,
      version::VERSION,
      "Expect version to be {}",
      version::VERSION
    );
    assert_eq!(
      data.commit_id,
      version::COMMIT_ID,
      "Expect commit_id to be {}",
      version::COMMIT_ID
    );
  }

  #[ntex::test]
  async fn ping() {
    let client = gen_test_client(ntex_config, version::VERSION).await;
    let res = client.send_head("/_ping", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::ACCEPTED, "ping");
  }

  #[ntex::test]
  async fn unhandled_route() {
    let client = gen_test_client(ntex_config, version::VERSION).await;
    let res = client.send_get("/v0.1/unhandled", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::NOT_FOUND, "unhandled");
  }
}
