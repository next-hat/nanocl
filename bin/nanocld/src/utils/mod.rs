pub mod ws;
pub mod key;
pub mod stream;

pub mod store;
pub mod system;
pub mod namespace;
pub mod cargo;
pub mod cargo_image;
pub mod vm;
pub mod vm_image;
pub mod job;
pub mod exec;
pub mod state;
pub mod proxy;
pub mod resource;
pub mod metric;
pub mod ctrl_client;

#[cfg(test)]
pub mod tests {
  use super::*;

  use std::fs;
  use std::env;
  use ntex::web::{*, self};

  use nanocl_stubs::config::DaemonConfig;

  use crate::version::VERSION;
  use crate::services;
  use crate::event::EventEmitter;
  use crate::models::{Pool, DaemonState};

  pub use ntex::web::test::TestServer;
  pub use nanocl_utils::ntex::test_client::*;

  type Config = fn(&mut ServiceConfig);

  /// ## Before
  ///
  /// Set the log level to info and build a test env logger for tests purpose
  ///
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

  /// ## Gen docker client
  ///
  /// Generate a docker client for tests purpose
  ///
  /// ## Return
  ///
  /// * [bollard_next::Docker](bollard_next::Docker) - The docker client
  ///
  pub fn gen_docker_client() -> bollard_next::Docker {
    let socket_path = env::var("DOCKER_SOCKET_PATH")
      .unwrap_or_else(|_| String::from("/var/run/docker.sock"));
    println!("Using docker socket path: {}", socket_path);
    bollard_next::Docker::connect_with_unix(
      &socket_path,
      120,
      bollard_next::API_DEFAULT_VERSION,
    )
    .unwrap()
  }

  /// ## Parse statefile
  ///
  /// Parse a state file from yaml to json format for tests purpose
  ///
  /// ## Arguments
  ///
  /// * [path](str) Path to the state file
  ///
  /// ## Return
  ///
  /// * [Result](Result) Result of the operation
  ///   * [Ok](serde_json::Value) - The state file parsed
  ///   * [Err](Box) - The state file has not been parsed
  ///
  pub fn parse_statefile(
    path: &str,
  ) -> Result<serde_json::Value, Box<dyn std::error::Error + 'static>> {
    let data = fs::read_to_string(path)?;
    let data: serde_yaml::Value = serde_yaml::from_str(&data)?;
    let data = serde_json::to_value(data)?;
    Ok(data)
  }

  /// ## Gen postgre pool
  ///
  /// Generate a postgre pool for tests purpose
  ///
  /// ## Return
  ///
  /// * [Pool](Pool) - The postgre pool
  ///
  pub async fn gen_postgre_pool() -> Pool {
    let home = std::env::var("HOME").expect("Failed to get home dir");
    let daemon_conf = DaemonConfig {
      state_dir: format!("{home}/.nanocl_dev/state"),
      ..Default::default()
    };
    store::create_pool("nstore.nanocl.internal:26258", &daemon_conf)
      .await
      .expect("Failed to connect to store at: {ip_addr}")
  }

  /// ## Gen server
  ///
  /// Generate a test server for tests purpose
  ///
  /// ## Arguments
  ///
  /// * [routes](Config) Routes to configure
  ///
  /// ## Return
  ///
  /// * [TestServer](TestServer) - The test server
  ///
  pub async fn gen_server(routes: Config) -> test::TestServer {
    before();
    // Build a test daemon config
    let home = env::var("HOME").expect("Failed to get home dir");
    let docker_host = env::var("DOCKER_SOCKET_PATH")
      .unwrap_or_else(|_| String::from("/var/run/docker.sock"));
    let config = DaemonConfig {
      state_dir: format!("{home}/.nanocl/state"),
      docker_host,
      ..Default::default()
    };
    let event_emitter = EventEmitter::new();
    // Create docker_api
    let docker_api = gen_docker_client();
    // Create postgres pool
    let pool = gen_postgre_pool().await;
    let daemon_state = DaemonState {
      config,
      docker_api,
      pool,
      event_emitter,
      version: VERSION.to_owned(),
    };
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
