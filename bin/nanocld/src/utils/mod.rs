pub(crate) mod ws;
pub(crate) mod key;
pub(crate) mod stream;

pub(crate) mod store;
pub(crate) mod system;
pub(crate) mod namespace;
pub(crate) mod cargo;
pub(crate) mod cargo_image;
pub(crate) mod vm;
pub(crate) mod vm_image;
pub(crate) mod job;
pub(crate) mod exec;
pub(crate) mod state;
pub(crate) mod proxy;
pub(crate) mod resource;
pub(crate) mod metric;
pub(crate) mod ctrl_client;
pub(crate) mod secret;
pub(crate) mod event;
pub(crate) mod process;

#[cfg(test)]
pub mod tests {
  use super::*;

  use std::{fs, env};
  use ntex::web::{*, self};

  pub use ntex::web::test::TestServer;

  use crate::{
    services,
    version::VERSION,
    event_emitter::EventEmitter,
    models::{Pool, DaemonState},
  };
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

  /// Generate a docker client for tests purpose
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

  /// Parse a state file from yaml to json format for tests purpose
  pub fn parse_statefile(
    path: &str,
  ) -> Result<serde_json::Value, Box<dyn std::error::Error + 'static>> {
    let data = fs::read_to_string(path)?;
    let data: serde_yaml::Value = serde_yaml::from_str(&data)?;
    let data = serde_json::to_value(data)?;
    Ok(data)
  }

  /// Generate a postgre pool for tests purpose
  pub async fn gen_postgre_pool() -> Pool {
    let home = std::env::var("HOME").expect("Failed to get home dir");
    let daemon_conf = DaemonConfig {
      state_dir: format!("{home}/.nanocl_dev/state"),
      ..Default::default()
    };
    store::create_pool("store.nanocl.internal:26258", &daemon_conf)
      .await
      .expect("Failed to connect to store at: {ip_addr}")
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
