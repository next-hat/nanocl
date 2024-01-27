pub mod ws;
pub mod key;
pub mod stream;

pub mod node;
pub mod store;
pub mod system;
pub mod namespace;
pub mod cargo;
pub mod container_image;
pub mod vm;
pub mod vm_image;
pub mod job;
pub mod exec;
pub mod resource;
pub mod ctrl_client;
pub mod process;
pub mod server;
pub mod container;

#[cfg(test)]
pub mod tests {
  use std::env;
  use ntex::web::{*, self};

  use crate::{services, vars::VERSION, models::SystemState};
  use nanocl_stubs::config::DaemonConfig;

  pub use nanocl_utils::ntex::test_client::*;

  type Config = fn(&mut ServiceConfig);

  /// Set the log level to info and build a test env logger for tests purpose
  pub fn before() {
    // Build a test env logger
    if std::env::var("LOG_LEVEL").is_err() {
      std::env::set_var("LOG_LEVEL", "nanocld=info,warn,error,nanocld=debug");
    }
    let _ = env_logger::Builder::new()
      .parse_env("LOG_LEVEL")
      .is_test(true)
      .try_init();
  }

  /// Generate a test server for tests purpose
  pub async fn gen_server(routes: Config) -> test::TestServer {
    before();
    // Build a test daemon config
    let home = env::var("HOME").expect("Failed to get home dir");
    let docker_host = env::var("DOCKER_SOCKET_PATH")
      .unwrap_or_else(|_| String::from("/var/run/docker.sock"));
    let config = DaemonConfig {
      state_dir: format!("{home}/.nanocl_dev/state"),
      docker_host,
      ..Default::default()
    };
    let daemon_state = SystemState::new(&config).await.unwrap();
    // Create test server
    test::server(move || {
      App::new()
        .state(daemon_state.clone())
        .configure(routes)
        .default_service(web::route().to(services::unhandled))
    })
  }

  pub async fn gen_test_client(routes: Config, version: &str) -> TestClient {
    let srv = gen_server(routes).await;
    TestClient::new(srv, version)
  }

  pub async fn gen_default_test_client() -> TestClient {
    let srv = gen_server(services::ntex_config).await;
    TestClient::new(srv, VERSION)
  }
}
