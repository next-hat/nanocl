use ntex::web;

pub mod info;
pub mod ping;
pub mod version;

pub use info::*;
pub use ping::*;
pub use version::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(get_ping);
  config.service(get_version);
  config.service(get_info);
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::system::HostInfo;
  use ntex::http;

  use crate::services::ntex_config;
  use crate::utils::tests::*;

  #[ntex::test]
  async fn system_info() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut res = client.send_get("/info", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "system info");
    let _ = res.json::<HostInfo>().await.unwrap();
  }

  #[ntex::test]
  async fn wrong_version() {
    let client = gen_test_system(ntex_config, "13.44").await.client;
    let res = client.send_get("/info", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "wrong version 13.44"
    );
    let client = gen_test_system(ntex_config, "5.2").await.client;
    let res = client.send_get("/info", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "wrong version 5.2"
    );
  }

  #[ntex::test]
  async fn ping() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client.send_head("/_ping", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::ACCEPTED, "ping");
  }
}
