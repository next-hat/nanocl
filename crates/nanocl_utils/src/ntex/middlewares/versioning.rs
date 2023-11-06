/// Versionning middleware
use std::rc::Rc;

use ntex::{Service, ServiceCtx, Middleware};
use futures::future::{ok, Either, Ready, LocalBoxFuture};
use ntex::web::{WebRequest, WebResponse, Error, ErrorRenderer, HttpResponse};

struct Inner {
  version: String,
}

/// Versioning middleware creator
///
/// ```no_run,ignore
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
        version: version.to_owned(),
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
  type Future<'f> = Either<LocalBoxFuture<'f, Result<Self::Response, S::Error>>, Ready<Result<Self::Response, S::Error>>> where Self: 'f;

  ntex::forward_poll_ready!(service);

  fn call<'a>(
    &'a self,
    mut req: WebRequest<Err>,
    ctx: ServiceCtx<'a, Self>,
  ) -> Self::Future<'_> {
    let version = req.match_info_mut().get("version");
    match version {
      None => {}
      Some(version) => {
        let version_number = version.replace('v', "");
        let versions = version_number.split('.').collect::<Vec<&str>>();
        let major = versions.first();
        let minor = versions.get(1);
        let current_versions =
          self.inner.version.split('.').collect::<Vec<&str>>();
        let current_major = current_versions.first();
        let current_minor = current_versions.get(1);

        if minor.is_none()
          || major.is_none()
          || current_major.is_none()
          || current_minor.is_none()
        {
          let msg = format!("{version} is invalid");
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

        let minor_number = minor.unwrap().parse::<usize>();
        let major_number = major.unwrap().parse::<usize>();
        let current_minor_number = current_minor.unwrap().parse::<usize>();
        let current_major_number = current_major.unwrap().parse::<usize>();

        if minor_number.is_err()
          || major_number.is_err()
          || current_minor_number.is_err()
          || current_major_number.is_err()
        {
          let msg = format!("{version} is not a number");
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

        let minor_number = minor_number.unwrap();
        let major_number = major_number.unwrap();
        let current_minor_number = current_minor_number.unwrap();
        let current_major_number = current_major_number.unwrap();

        if major_number > current_major_number
          || (major_number == current_major_number
            && minor_number > current_minor_number)
        {
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

    Either::Left(Box::pin(async move { ctx.call(&self.service, req).await }))
  }
}
