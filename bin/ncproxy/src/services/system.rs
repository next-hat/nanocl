use ntex::web;

use nanocl_error::http::HttpError;

use crate::version;

/// Get version information
#[cfg_attr(feature = "dev", utoipa::path(
  head,
  tag = "System",
  path = "/_ping",
  responses(
    (status = 202, description = "Server is up"),
  ),
))]
#[web::head("/_ping")]
pub(crate) async fn head_ping() -> Result<web::HttpResponse, HttpError> {
  Ok(web::HttpResponse::Accepted().into())
}

/// Get version information
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "System",
  path = "/version",
  responses(
    (status = 200, description = "Version information", body = Version),
  ),
))]
#[web::get("/version")]
pub(crate) async fn get_version() -> web::HttpResponse {
  web::HttpResponse::Ok().json(&serde_json::json!({
    "Arch": version::ARCH,
    "Channel": version::CHANNEL,
    "Version": version::VERSION,
    "CommitId": version::COMMIT_ID,
  }))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(head_ping);
  config.service(get_version);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use nanocld_client::stubs::system::Version;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn head_ping() {
    let client = gen_default_test_client().await;
    let res = client.send_head("/_ping", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::ACCEPTED, "ping");
  }

  #[ntex::test]
  async fn get_version() {
    let client = gen_default_test_client().await;
    let mut res = client.send_get("/version", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "get version");
    let _ = res.json::<Version>().await.unwrap();
  }
}
