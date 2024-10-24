use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  resource::ResourceSpec,
};

use crate::{
  models::{SpecDb, SystemState},
  repositories::generic::*,
};

/// List resource history
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{name}/histories",
  params(
    ("name" = String, Path, description = "The resource name to list history")
  ),
  responses(
    (status = 200, description = "The resource history", body = [ResourceSpec]),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::get("/resources/{name}/histories")]
pub async fn list_resource_history(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let filter =
    GenericFilter::new().r#where("kind_key", GenericClause::Eq(path.1.clone()));
  let items = SpecDb::read_by(&filter, &state.inner.pool)
    .await?
    .into_iter()
    .map(ResourceSpec::from)
    .collect::<Vec<_>>();
  Ok(web::HttpResponse::Ok().json(&items))
}
