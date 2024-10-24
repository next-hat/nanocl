use ntex::web;

use nanocl_error::http::HttpResult;

/// Ping the server to check if it is up
#[cfg_attr(feature = "dev", utoipa::path(
  head,
  tag = "System",
  path = "/_ping",
  responses(
    (status = 202, description = "Server is up"),
  ),
))]
#[web::head("/_ping")]
pub async fn get_ping() -> HttpResult<web::HttpResponse> {
  Ok(web::HttpResponse::Accepted().into())
}
