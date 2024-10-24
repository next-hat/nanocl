use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::resource::ResourcePartial;

use crate::{
  models::{ResourceDb, SystemState},
  objects::generic::*,
};

/// Create a new resource
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = ResourcePartial,
  tag = "Resources",
  path = "/resources",
  responses(
    (status = 200, description = "The created resource", body = Resource),
  ),
))]
#[web::post("/resources")]
pub async fn create_resource(
  state: web::types::State<SystemState>,
  payload: web::types::Json<ResourcePartial>,
) -> HttpResult<web::HttpResponse> {
  let resource = ResourceDb::create_obj(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&resource))
}
