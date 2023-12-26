pub mod server;
pub mod rule;
pub mod nginx;
pub mod resource;

#[cfg(test)]
pub(crate) mod tests {
  use std::sync::Arc;

  use nanocl_utils::logger;

  pub use nanocl_utils::ntex::test_client::*;

  use crate::{variables, services};

  // Before a test
  pub fn before() {
    // Build a test env logger
    std::env::set_var("TEST", "true");
    logger::enable_logger("ncproxy");
  }

  pub async fn gen_default_test_client() -> TestClient {
    before();
    let home = std::env::var("HOME").unwrap();
    let options = crate::cli::Cli {
      state_dir: format!("{home}/proxy"),
      nginx_dir: "/etc/nginx".to_owned(),
    };
    let system_state = crate::subsystem::init(&options).await.unwrap();
    // Create test server
    let srv = ntex::web::test::server(move || {
      ntex::web::App::new()
        .state(Arc::clone(&system_state))
        .configure(services::ntex_config)
    });
    TestClient::new(srv, variables::VERSION)
  }
}
