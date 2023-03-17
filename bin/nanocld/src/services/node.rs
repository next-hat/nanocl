use ntex::web;

use crate::repositories;
use crate::error::HttpResponseError;
use crate::models::DaemonState;

#[web::get("/nodes")]
async fn list_node(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let items = repositories::node::list(&state.pool).await?;

  Ok(web::HttpResponse::Ok().json(&items))
}
