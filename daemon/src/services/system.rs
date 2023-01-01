use ntex::web;
use serde_json::json;

use crate::version;

#[web::get("/version")]
async fn get_version() -> web::HttpResponse {
  web::HttpResponse::Ok().json(&json!({
    "arch": version::ARCH,
    "version": version::VERSION,
    "commit_id": version::COMMIT_ID,
  }))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(get_version);
}

#[cfg(test)]
mod tests {
  use ntex::http::StatusCode;

  use super::*;
  use crate::{utils::tests::*, models::Version};

  #[ntex::test]
  pub async fn get_version() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let mut resp = srv.get("/version").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    let body: Version = resp
      .json()
      .await
      .expect("To receive a valid version json payload");
    assert_eq!(
      body.arch,
      version::ARCH,
      "Expect arch to be {}",
      version::ARCH
    );
    assert_eq!(
      body.version,
      version::VERSION,
      "Expect version to be {}",
      version::VERSION
    );
    assert_eq!(
      body.commit_id,
      version::COMMIT_ID,
      "Expect commit_id to be {}",
      version::COMMIT_ID
    );
    Ok(())
  }
}
