use futures::StreamExt;
use ntex::util::BytesMut;
use ntex::{Ctx, Middleware, Service, http, web};

/// Middleware to convert default ntex SerializeError from text/plain to application/json
pub struct SerializeError;

impl<S, St> Middleware<S, St> for SerializeError {
  type Service = SerializeErrorMiddleware<S>;

  fn create(&self, _: &St, service: S) -> Self::Service {
    SerializeErrorMiddleware { service }
  }
}

pub struct SerializeErrorMiddleware<S> {
  service: S,
}

impl<S, St, In> Service<St, web::WebRequest<In>> for SerializeErrorMiddleware<S>
where
  S: Service<St, web::WebRequest<In>, Res = web::WebResponse>,
{
  type Res = web::WebResponse;
  type Error = S::Error;

  ntex::forward_ready!(St, service);
  ntex::forward_shutdown!(St, service);

  async fn call(
    &self,
    req: web::WebRequest<In>,
    ctx: Ctx<'_, Self, St>,
  ) -> Result<Self::Res, Self::Error> {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ntex4_serialized_errors_preserve_outer_headers_and_state() {
    futures::executor::block_on(async {
      let app = web::test::init_service_st(
        web::AppState::new("accepted"),
        web::App::<web::AppState<&'static str>>::new()
          .middleware(
            web::middleware::DefaultHeaders::new()
              .header("x-api-version", "1.2"),
          )
          .middleware(SerializeError)
          .route(
            "/test",
            web::post().to_with_state(
              async |state: &web::AppState<&'static str>,
                     (): (),
                     _: web::types::Json<serde_json::Value>| {
                web::HttpResponse::Ok().body(*state.st())
              },
            ),
          )
          .build(),
      )
      .await;

      let request = web::test::TestRequest::post()
        .uri("/test")
        .header(http::header::CONTENT_TYPE, "application/json")
        .payload("{")
        .to_request();
      let response = app.call(request).await.unwrap();
      assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
      assert_eq!(response.headers().get("x-api-version").unwrap(), "1.2");
      assert_eq!(
        response.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/json",
      );
      let body = web::test::read_body(response).await;
      let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
      assert!(error["msg"].as_str().unwrap().starts_with("payload"));

      let request = web::test::TestRequest::post()
        .uri("/test")
        .json(&serde_json::json!({}))
        .to_request();
      let response = app.call(request).await.unwrap();
      assert_eq!(response.status(), http::StatusCode::OK);
      assert_eq!(web::test::read_body(response).await, "accepted");
    });
  }
}
