use ntex::web;
use nanocl_utils::ntex::middlewares;
use nanocl_utils::http_error::HttpError;

use crate::version;

#[cfg(feature = "dev")]
mod openapi;

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

  let versionning = middlewares::Versioning::new(version::VERSION).finish();

  config.service(
    web::scope("/{version}")
      .wrap(versionning)
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
      .configure(http_metric::ntex_config),
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::version;
  use ntex::http::StatusCode;

  use nanocl_stubs::system::Version;

  use crate::utils::tests::*;

  #[ntex::test]
  pub async fn get_version() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let mut resp = srv.get("/v0.5/version").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    let body: Version = resp
      .json()
      .await
      .expect("To receive a valid version json payload");
    assert_eq!(
      body.arch,
      version::ARCH,
      "Expect arch to be {}",
      version::ARCH
    );
    assert_eq!(
      body.version,
      version::VERSION,
      "Expect version to be {}",
      version::VERSION
    );
    assert_eq!(
      body.commit_id,
      version::COMMIT_ID,
      "Expect commit_id to be {}",
      version::COMMIT_ID
    );
    Ok(())
  }

  #[ntex::test]
  async fn test_ping() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let resp = srv.head("/v0.5/_ping").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::ACCEPTED,
      "Expect status to be {} got {}",
      StatusCode::ACCEPTED,
      status
    );
    Ok(())
  }

  #[ntex::test]
  async fn test_unhandled_route() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let resp = srv.get("/unhandled").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::NOT_FOUND,
      "Expect status to be {} got {}",
      StatusCode::NOT_FOUND,
      status
    );
    Ok(())
  }
}
