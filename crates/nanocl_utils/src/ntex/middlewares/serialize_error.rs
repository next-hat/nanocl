use futures::StreamExt;
use ntex::util::BytesMut;
use ntex::{Middleware, Service, ServiceCtx, http, web};

/// Middleware to convert default ntex SerializeError from text/plain to application/json
pub struct SerializeError;

impl<S, Cfg> Middleware<S, Cfg> for SerializeError {
  type Service = SerializeErrorMiddleware<S>;

  fn create(&self, service: S, _: Cfg) -> Self::Service {
    SerializeErrorMiddleware { service }
  }
}

pub struct SerializeErrorMiddleware<S> {
  service: S,
}

impl<S, Err> Service<web::WebRequest<Err>> for SerializeErrorMiddleware<S>
where
  S: Service<
      web::WebRequest<Err>,
      Response = web::WebResponse,
      Error = web::Error,
    >,
  Err: web::ErrorRenderer,
{
  type Response = web::WebResponse;
  type Error = web::Error;

  ntex::forward_ready!(service);
  ntex::forward_shutdown!(service);

  async fn call(
    &self,
    req: web::WebRequest<Err>,
    ctx: ServiceCtx<'_, Self>,
  ) -> Result<Self::Response, Self::Error> {
    let mut res = ctx.call(&self.service, req).await?;
    if res.status() == http::StatusCode::BAD_REQUEST {
      let content_type = res.headers().get(http::header::CONTENT_TYPE);
      if let Some(content_type) = content_type
        && content_type == "text/plain; charset=utf-8"
      {
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
    Ok(res)
  }
}
