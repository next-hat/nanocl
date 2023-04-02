pub mod ws;
pub mod key;
pub mod stream;
pub mod network;

pub mod store;
pub mod state;
pub mod proxy;
pub mod resource;
pub mod namespace;
pub mod vm;
pub mod vm_image;
pub mod cargo;
pub mod cargo_image;

#[cfg(test)]
pub mod tests {
  use super::*;

  use std::fs;
  use std::env;
  use ntex::web::{*, self};
  use ntex::http::StatusCode;
  use ntex::http::client::ClientResponse;
  use ntex::http::client::error::SendRequestError;

  use nanocl_stubs::config::DaemonConfig;

  use crate::error::HttpResponseError;
  use crate::models::DaemonState;
  use crate::services;
  use crate::event::EventEmitter;
  use crate::models::Pool;
  use crate::version::VERSION;

  pub use ntex::web::test::TestServer;
  pub type TestReqRet = Result<ClientResponse, SendRequestError>;
  pub type TestRet = Result<(), Box<dyn std::error::Error + 'static>>;

  type Config = fn(&mut ServiceConfig);

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

  /// ## Get store ip address
  ///
  /// Get the ip address of the store container
  ///
  /// ## Arguments
  ///
  /// [docker_api](Docker) Reference to docker api
  ///
  /// ## Returns
  ///
  /// - [Result](Result) Result of the operation
  ///   - [Ok](String) - The ip address of the store
  ///   - [Err](HttpResponseError) - The ip address of the store has not been retrieved
  ///
  /// ## Example
  ///
  /// ```rust,norun
  /// use crate::utils;
  ///
  /// let docker_api = Docker::connect_with_local_defaults().unwrap();
  /// let ip_address = utils::store::get_store_ip_addr(&docker_api).await;
  /// ```
  ///
  pub async fn get_store_ip_addr(
    docker_api: &bollard_next::Docker,
  ) -> Result<String, HttpResponseError> {
    let container =
      docker_api.inspect_container("store.system.c", None).await?;
    let networks = container
      .network_settings
      .ok_or(HttpResponseError {
        msg: String::from("unable to get store network nettings"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?
      .networks
      .ok_or(HttpResponseError {
        msg: String::from("unable to get store networks"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
    let ip_address = networks
      .get("system")
      .ok_or(HttpResponseError {
        msg: String::from("unable to get store network nanocl"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?
      .ip_address
      .as_ref()
      .ok_or(HttpResponseError {
        msg: String::from("unable to get store network nanocl"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
    Ok(ip_address.to_owned())
  }

  pub fn gen_docker_client() -> bollard_next::Docker {
    let socket_path = env::var("DOCKER_SOCKET_PATH")
      .unwrap_or_else(|_| String::from("/run/docker.sock"));
    bollard_next::Docker::connect_with_unix(
      &socket_path,
      120,
      bollard_next::API_DEFAULT_VERSION,
    )
    .unwrap()
  }

  pub fn parse_state_file(
    path: &str,
  ) -> Result<serde_json::Value, Box<dyn std::error::Error + 'static>> {
    let data = fs::read_to_string(path)?;
    let data: serde_yaml::Value = serde_yaml::from_str(&data)?;
    let data = serde_json::to_value(data)?;
    Ok(data)
  }

  pub async fn gen_postgre_pool() -> Pool {
    let docker_api = gen_docker_client();
    let ip_addr = get_store_ip_addr(&docker_api).await.unwrap();

    store::create_pool(ip_addr).await
  }

  pub async fn generate_server(routes: Config) -> test::TestServer {
    before();
    // Build a test daemon config
    let config = DaemonConfig {
      state_dir: String::from("/var/lib/nanocl"),
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
}
