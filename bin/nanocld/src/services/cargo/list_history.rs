use ntex::web;

use nanocl_error::{http::HttpResult, io::IoResult};

use crate::{
  models::{SpecDb, SystemState},
  utils,
};

/// List cargo histories
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{key}/histories",
  params(
    ("key" = String, Path, description = "Canonical cargo key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 200, description = "List of cargo histories", body = Vec<nanocl_stubs::cargo_spec::CargoSpecRevision>),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::get("/cargoes/{key}/histories")]
pub async fn list_cargo_history(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let histories = SpecDb::read_by_kind_key(key.as_str(), &state.inner.pool)
    .await?
    .into_iter()
    .filter(|history| history.kind_name == "Cargo")
    .map(|e| e.try_to_cargo_spec_revision())
    .collect::<IoResult<Vec<_>>>()?;
  Ok(web::HttpResponse::Ok().json(&histories))
}
