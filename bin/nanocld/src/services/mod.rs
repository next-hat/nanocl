use ntex::{web, Service, Middleware};
use ntex::web::{WebRequest, WebResponse, Error, ErrorRenderer, HttpResponse};
use futures::future::{ok, Either, Ready};

use crate::version;
use crate::error::HttpResponseError;

mod state;
mod node;
mod namespace;
mod system;
mod metric;
mod resource;
mod cargo;
mod cargo_image;
mod vm;
mod vm_image;

pub struct Versionning;

impl<S> Middleware<S> for Versionning {
  type Service = VersionningMiddleware<S>;

  fn create(&self, service: S) -> Self::Service {
    VersionningMiddleware { service }
  }
}

pub struct VersionningMiddleware<S> {
  service: S,
}

impl<S, Err> Service<WebRequest<Err>> for VersionningMiddleware<S>
where
  S: Service<WebRequest<Err>, Response = WebResponse, Error = Error>,
  Err: ErrorRenderer,
{
  type Response = WebResponse;
  type Error = Error;
  type Future<'f> = Either<S::Future<'f>, Ready<Result<Self::Response, Self::Error>>> where Self: 'f;

  ntex::forward_poll_ready!(service);

  fn call(&self, mut req: WebRequest<Err>) -> Self::Future<'_> {
    let version = req.match_info_mut().get("version");
    match version {
      None => {}
      Some(version) => {
        if version.replace('v', "").as_str() > version::VERSION {
          let msg = format!("{version} is not supported");
          return Either::Right(ok(
            req.into_response(
              HttpResponse::NotFound()
                .json(&serde_json::json!({
                  "msg": msg,
                }))
                .into_body(),
            ),
          ));
        }
      }
    }
    Either::Left(self.service.call(req))
  }
}

// This is a dummy endpoint you can use to test if the server is accessible.
async fn ping() -> Result<web::HttpResponse, HttpResponseError> {
  Ok(web::HttpResponse::Ok().json(&serde_json::json!({
    "msg": "pong",
  })))
}

#[web::get("/version")]
async fn get_version() -> web::HttpResponse {
  web::HttpResponse::Ok().json(&serde_json::json!({
    "Arch": version::ARCH,
    "Channel": version::CHANNEL,
    "Version": version::VERSION,
    "CommitId": version::COMMIT_ID,
  }))
}

pub async fn unhandled() -> Result<web::HttpResponse, HttpResponseError> {
  Err(HttpResponseError {
    status: ntex::http::StatusCode::NOT_FOUND,
    msg: "Route or method unhandled".into(),
  })
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config
    .service(
      web::resource("/_ping")
        .route(web::get().to(ping))
        .route(web::head().to(ping)),
    )
    .service(get_version);

  config.service(
    web::scope("/{version}")
      .wrap(Versionning)
      .configure(state::ntex_config)
      .configure(namespace::ntex_config)
      .configure(system::ntex_config)
      .configure(resource::ntex_config)
      .configure(cargo_image::ntex_config)
      .configure(cargo::ntex_config)
      .configure(vm_image::ntex_config)
      .configure(vm::ntex_config)
      .configure(node::ntex_config)
      .configure(metric::ntex_config),
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
    let mut resp = srv.get("/version").send().await?;
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
    let mut resp = srv.get("/_ping").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    let body: serde_json::Value = resp
      .json()
      .await
      .expect("To receive a valid version json payload");
    assert_eq!(body["msg"], "pong");

    let resp = srv.head("/_ping").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
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
