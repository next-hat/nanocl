use ntex::web;

use crate::vars;

/// Get version information
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "System",
  path = "/version",
  responses(
    (status = 200, description = "Version information", body = nanocl_stubs::system::BinaryInfo),
  ),
))]
pub async fn get_version() -> web::HttpResponse {
  web::HttpResponse::Ok().json(&serde_json::json!({
    "Arch": vars::ARCH,
    "Channel": vars::CHANNEL,
    "Version": vars::VERSION,
    "CommitId": vars::COMMIT_ID,
  }))
}
