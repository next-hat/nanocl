use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

#[cfg(feature = "dev")]
pub mod openapi;

mod cargo;
mod event;
mod exec;
mod job;
mod metric;
mod namespace;
mod node;
mod process;
mod resource;
mod resource_kind;
mod secret;
mod system;
mod vm;
mod vm_image;

pub async fn unhandled() -> HttpResult<web::HttpResponse> {
  Err(HttpError::not_found("Route or method unhandled"))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(
    web::scope("/{version}")
      .wrap(
        nanocl_utils::ntex::middlewares::Versioning::new(crate::vars::VERSION)
          .finish(),
      )
      .configure(exec::ntex_config)
      .configure(node::ntex_config)
      .configure(namespace::ntex_config)
      .configure(system::ntex_config)
      .configure(resource::ntex_config)
      .configure(cargo::ntex_config)
      .configure(vm_image::ntex_config)
      .configure(vm::ntex_config)
      .configure(metric::ntex_config)
      .configure(secret::ntex_config)
      .configure(process::ntex_config)
      .configure(job::ntex_config)
      .configure(event::ntex_config)
      .configure(resource_kind::ntex_config),
  );
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use nanocl_stubs::system::BinaryInfo;

  use super::ntex_config;

  use crate::utils::tests::*;
  use crate::vars;

  #[ntex::test]
  pub async fn get_version() {
    let client = gen_test_system(ntex_config, vars::VERSION).await.client;
    let mut res = client.send_get("/version", None::<String>).await;
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
