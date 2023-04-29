/// Versionning middleware
use std::rc::Rc;

use ntex::{Service, Middleware};
use futures::future::{ok, Either, Ready};
use ntex::web::{WebRequest, WebResponse, Error, ErrorRenderer, HttpResponse};

struct Inner {
  version: String,
}

/// Versioning middleware creator
///
/// ```rust,norun
/// use ntex::web;
/// use ntex::middleware::Versioning;
///
/// let versioning = Versioning::new("1.0").finish();
///
/// web::scope("/{version}")
///  .wrap(versioning)
///  .route("/test", web::get().to(|| async { "test" }));
/// ```
pub struct Versioning {
  config: Inner,
}

impl Versioning {
  pub fn new(version: &str) -> Self {
    Self {
      config: Inner {
        version: version.to_string(),
      },
    }
  }

  pub fn finish(self) -> VersioningFactory {
    VersioningFactory {
      inner: Rc::new(self.config),
    }
  }
}

pub struct VersioningFactory {
  inner: Rc<Inner>,
}

impl<S> Middleware<S> for VersioningFactory {
  type Service = VersioningMiddleware<S>;

  fn create(&self, service: S) -> Self::Service {
    VersioningMiddleware {
      service,
      inner: self.inner.clone(),
    }
  }
}

pub struct VersioningMiddleware<S> {
  service: S,
  inner: Rc<Inner>,
}

impl<S, Err> Service<WebRequest<Err>> for VersioningMiddleware<S>
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
        if version.replace('v', "").as_str() > self.inner.version.as_str() {
          let msg = format!("{version} is not supported");
          return Either::Right(ok(
            req.into_response(
              HttpResponse::VersionNotSupported()
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
