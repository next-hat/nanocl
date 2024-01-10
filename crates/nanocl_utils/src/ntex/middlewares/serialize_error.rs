use ntex::{web, http};
use ntex::util::BytesMut;
use ntex::{Service, Middleware, ServiceCtx};
use ntex::web::{WebRequest, WebResponse, Error, ErrorRenderer};
use futures::StreamExt;

/// Middleware to convert default ntex SerializeError from text/plain to application/json
pub struct SerializeError;

impl<S> Middleware<S> for SerializeError {
  type Service = SerializeErrorMiddleware<S>;

  fn create(&self, service: S) -> Self::Service {
    SerializeErrorMiddleware { service }
  }
}

pub struct SerializeErrorMiddleware<S> {
  service: S,
}

impl<S, Err> Service<WebRequest<Err>> for SerializeErrorMiddleware<S>
where
  S: Service<WebRequest<Err>, Response = WebResponse, Error = Error>,
  Err: ErrorRenderer,
{
  type Response = WebResponse;
  type Error = Error;

  ntex::forward_poll_ready!(service);

  async fn call<'a>(
    &self,
    req: WebRequest<Err>,
    ctx: ServiceCtx<'a, Self>,
  ) -> Result<Self::Response, Self::Error> {
    let mut res = ctx.call(&self.service, req).await?;
    if res.status() == http::StatusCode::BAD_REQUEST {
      let content_type = res.headers().get(http::header::CONTENT_TYPE);
      if let Some(content_type) = content_type {
        if content_type == "text/plain; charset=utf-8" {
          let mut payload = BytesMut::new();
          let mut body = res.take_body();
          while let Some(chunk) = body.next().await {
            let chunk = chunk.unwrap_or_default();
            payload.extend_from_slice(&chunk);
          }
          res = res.into_response(web::HttpResponse::BadRequest().json(
              &serde_json::json!({
                "msg": &String::from_utf8_lossy(&payload).replace("Json deserialize error:", "payload"),
              }),
            ));
        }
      }
    }
    Ok(res)
  }
}
