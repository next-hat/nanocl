use ntex::web;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoResult,
};
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{SystemState, VmDb},
  utils,
};

fn history_pagination(
  query: &GenericListQuery,
) -> HttpResult<(Option<i64>, Option<i64>)> {
  let filter = utils::query_string::parse_qs_filter(query)?;
  if filter.r#where.is_some() || filter.order_by.is_some() {
    return Err(HttpError::bad_request(
      "VM history supports only limit and offset filters",
    ));
  }
  let limit = filter.limit.map(i64::try_from).transpose().map_err(|_| {
    HttpError::bad_request("VM history limit exceeds the supported range")
  })?;
  let offset = filter.offset.map(i64::try_from).transpose().map_err(|_| {
    HttpError::bad_request("VM history offset exceeds the supported range")
  })?;
  Ok((limit, offset))
}

/// List virtual machine histories
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{key}/histories",
  params(
    ("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format"),
    ("filter" = Option<String>, Query, description = "Optional pagination JSON with limit and offset only. Omit pagination to return the complete history."),
  ),
  responses(
    (status = 200, description = "The virtual machine histories have been listed", body = [nanocl_stubs::vm_spec::VmSpec]),
    (status = 400, description = "Invalid pagination or unsupported filter", body = crate::services::openapi::ApiError),
    (status = 404, description = "The virtual machine does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn list_vm_history(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  query: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let (limit, offset) = history_pagination(&query)?;
  let histories = VmDb::history(key.as_str(), limit, offset, &state.inner.pool)
    .await?
    .into_iter()
    .map(|i| i.try_to_vm_spec())
    .collect::<IoResult<Vec<_>>>()?;
  Ok(web::HttpResponse::Ok().json(&histories))
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_stubs::generic::GenericFilter;

  #[test]
  fn vm_history_pagination_preserves_full_default_and_bounds_pages() {
    assert_eq!(
      history_pagination(&GenericListQuery::default()).unwrap(),
      (None, None),
    );
    let query =
      GenericListQuery::try_from(GenericFilter::new().limit(100).offset(20))
        .unwrap();
    assert_eq!(history_pagination(&query).unwrap(), (Some(100), Some(20)));
    let query = GenericListQuery {
      filter: Some("{\"limit\":0}".to_owned()),
    };
    assert_eq!(history_pagination(&query).unwrap(), (Some(0), None));
  }

  #[test]
  fn vm_history_pagination_rejects_unsupported_and_invalid_filters() {
    for filter in [
      r#"{"where":{}}"#,
      r#"{"order_by":["key asc"]}"#,
      r#"{"limit":-1}"#,
      r#"{"offset":-1}"#,
      r#"{"limit":18446744073709551615}"#,
      r#"{"offset":18446744073709551615}"#,
      "invalid",
    ] {
      let query = GenericListQuery {
        filter: Some(filter.to_owned()),
      };
      assert_eq!(
        history_pagination(&query).unwrap_err().status,
        ntex::http::StatusCode::BAD_REQUEST,
        "{filter}",
      );
    }
  }
}
