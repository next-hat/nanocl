use ntex::web;

use crate::{
  models::{ResourceDb, SystemState},
  utils,
};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::generic::GenericListQuery;

fn history_pagination(
  query: &GenericListQuery,
) -> HttpResult<(Option<i64>, Option<i64>)> {
  let filter = utils::query_string::parse_qs_filter(query)?;
  if filter.r#where.is_some() || filter.order_by.is_some() {
    return Err(HttpError::bad_request(
      "Resource history supports only limit and offset filters",
    ));
  }
  let limit = filter.limit.map(i64::try_from).transpose().map_err(|_| {
    HttpError::bad_request("Resource history limit exceeds the supported range")
  })?;
  let offset = filter.offset.map(i64::try_from).transpose().map_err(|_| {
    HttpError::bad_request(
      "Resource history offset exceeds the supported range",
    )
  })?;
  Ok((limit, offset))
}

/// List resource history
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{name}/histories",
  params(
    ("name" = String, Path, description = "The resource name to list history"),
    ("filter" = Option<String>, Query, description = "Optional pagination JSON with limit and offset only. Omit pagination to return the complete history."),
  ),
  responses(
    (status = 200, description = "The resource history", body = [nanocl_stubs::resource::ResourceSpec]),
    (status = 400, description = "Invalid pagination or unsupported filter", body = crate::services::openapi::ApiError),
    (status = 404, description = "Resource is not existing", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn list_resource_history(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  query: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let (limit, offset) = history_pagination(&query)?;
  let items =
    ResourceDb::history(&path.1, limit, offset, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_stubs::generic::GenericFilter;

  #[test]
  fn resource_history_pagination_preserves_full_default_and_bounds_pages() {
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
  fn resource_history_pagination_rejects_unsupported_and_invalid_filters() {
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
