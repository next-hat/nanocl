pub mod key;
pub(crate) mod operation;
pub mod stream;
pub mod ws;

pub mod container;
pub mod cron;
pub mod ctrl_client;
pub mod query_string;
pub mod secret;
pub mod server;
pub mod store;
pub mod system;

#[cfg(test)]
pub mod tests {
  use ntex::web::{self, *};
  use std::{env, sync::LazyLock};

  use crate::{models::SystemState, services, vars::VERSION};
  use nanocl_stubs::config::DaemonConfig;

  pub use nanocl_utils::ntex::test_client::*;

  type Config = fn(&mut ServiceConfig<SystemState>);

  static CONTROLLER_ID: LazyLock<uuid::Uuid> =
    LazyLock::new(uuid::Uuid::new_v4);

  pub struct TestSystem {
    pub state: SystemState,
    pub client: TestClient,
  }

  /// Set the log level to info and build a test env logger for tests purpose
  pub fn before() {
    // Build a test env logger
    unsafe {
      std::env::set_var("TEST", "true");
    }
  }

  pub async fn gen_test_system(routes: Config, version: &str) -> TestSystem {
    before();
    // Build a test daemon config
    let home = env::var("HOME").expect("Failed to get home dir");
    let docker_host = env::var("DOCKER_SOCKET_PATH")
      .unwrap_or_else(|_| String::from("/var/run/docker.sock"));
    let state_dir = env::var("NANOCL_TEST_STATE_DIR")
      .unwrap_or_else(|_| format!("{home}/.nanocl_dev/state"));
    // In-process test controllers must not share the external daemon's
    // exclusive installation lock. Keep their disposable state isolated while
    // using the existing database and Docker test infrastructure.
    let state_dir = format!("{state_dir}/test-controllers/{}", *CONTROLLER_ID);
    std::fs::create_dir_all(&state_dir).unwrap();
    let store_addr = env::var("NANOCL_TEST_STORE_ADDR").unwrap_or_else(|_| {
      "postgresql://root:root@store.nanocl.internal:26258/defaultdb".to_owned()
    });
    let config = DaemonConfig {
      state_dir,
      docker_host,
      hostname: "nanocl.internal".to_owned(),
      store_addr: Some(store_addr),
      ..Default::default()
    };
    let state = SystemState::new(&config).await.unwrap();
    let state_ptr = state.clone();
    // Create test server
    let srv = test::server(async move |_| {
      App::<SystemState>::new()
        .with_config(services::app_config())
        .configure(routes)
        .default_service(web::route().to(services::unhandled))
        .build_with(state_ptr.clone())
    });
    let client = TestClient::new(srv, version);
    TestSystem { state, client }
  }

  pub async fn gen_default_test_system() -> TestSystem {
    gen_test_system(services::ntex_config, VERSION).await
  }
}
