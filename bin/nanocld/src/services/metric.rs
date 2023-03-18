use ntex::web;

use crate::repositories;
use crate::models::DaemonState;
use crate::error::HttpResponseError;

#[web::get("/metrics")]
async fn list_metric(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  repositories::metric::list(&state.pool).await?;
  Ok(web::HttpResponse::Ok().into())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_metric);
}
