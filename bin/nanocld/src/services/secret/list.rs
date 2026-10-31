use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{SecretDb, SystemState},
  repositories::generic::*,
  utils,
};

/// List secret with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Secrets",
  path = "/secrets",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"kind\": { \"eq\": \"Env\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of secret", body = [nanocl_stubs::secret::Secret]),
  ),
))]
pub async fn list_secret(
  state: &SystemState,
  _request_state: (),
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let items = SecretDb::transform_read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}
