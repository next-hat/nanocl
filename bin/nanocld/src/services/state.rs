use ntex::web;

use crate::error::HttpResponseError;

#[web::patch("/state/apply")]
async fn apply() -> Result<web::HttpResponse, HttpResponseError> {
  Ok(web::HttpResponse::Ok().finish())
}
