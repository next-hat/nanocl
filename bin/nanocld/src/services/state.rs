use ntex::web;
use ntex::util::BytesMut;

use crate::error::HttpResponseError;

#[web::put("/state/apply")]
async fn apply(
  mut payload: web::types::Payload,
) -> Result<web::HttpResponse, HttpResponseError> {
  let mut body = BytesMut::new();
  while let Some(Ok(item)) = ntex::util::stream_recv(&mut payload).await {
    body.extend_from_slice(&item);
  }

  println!("body: {body:?}");

  Ok(web::HttpResponse::Ok().finish())
}

#[web::put("/state/revert")]
async fn revert() -> Result<web::HttpResponse, HttpResponseError> {
  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(cfg: &mut web::ServiceConfig) {
  cfg.service(apply);
  cfg.service(revert);
}
