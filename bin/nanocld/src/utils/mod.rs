pub mod ws;
pub mod key;
pub mod stream;

pub mod store;
pub mod system;
pub mod vm_image;
pub mod cron;
pub mod exec;
pub mod ctrl_client;
pub mod server;
pub mod container;
pub mod query_string;

#[cfg(test)]
pub mod tests {
  use std::env;
  use ntex::web::{*, self};

  use nanocl_stubs::config::DaemonConfig;
  use crate::{services, vars::VERSION, models::SystemState};

  pub use nanocl_utils::ntex::test_client::*;

  type Config = fn(&mut ServiceConfig);

  pub struct TestSystem {
    pub state: SystemState,
    pub client: TestClient,
  }

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

  pub async fn gen_test_system(routes: Config, version: &str) -> TestSystem {
    before();
    // Build a test daemon config
    let home = env::var("HOME").expect("Failed to get home dir");
    let docker_host = env::var("DOCKER_SOCKET_PATH")
      .unwrap_or_else(|_| String::from("/var/run/docker.sock"));
    let config = DaemonConfig {
      state_dir: format!("{home}/.nanocl_dev/state"),
      docker_host,
      hostname: "nanocl.internal".to_owned(),
      store_addr: Some(
        "postgresql://root:root@store.nanocl.internal:26258/defaultdb"
          .to_owned(),
      ),
      ..Default::default()
    };
    let state = SystemState::new(&config).await.unwrap();
    let state_ptr = state.clone();
    // Create test server
    let srv = test::server(move || {
      App::new()
        .state(state_ptr.clone())
        .configure(routes)
        .default_service(web::route().to(services::unhandled))
    });
    let client = TestClient::new(srv, version);
    TestSystem { state, client }
  }

  pub async fn gen_default_test_system() -> TestSystem {
    gen_test_system(services::ntex_config, VERSION).await
  }
}
