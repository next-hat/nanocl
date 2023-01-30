pub mod key;
pub mod store;
pub mod state;
pub mod cargo;
pub mod cargo_image;
pub mod namespace;

#[cfg(test)]
pub mod tests {
  use super::*;

  use std::env;
  use ntex::web::*;
  use ntex::http::client::ClientResponse;
  use ntex::http::client::error::SendRequestError;

  use nanocl_models::config::DaemonConfig;

  use crate::event::EventEmitter;
  use crate::models::Pool;

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

  pub fn gen_docker_client() -> bollard::Docker {
    let socket_path = env::var("DOCKER_SOCKET_PATH")
      .unwrap_or_else(|_| String::from("/run/docker.sock"));
    bollard::Docker::connect_with_unix(
      &socket_path,
      120,
      bollard::API_DEFAULT_VERSION,
    )
    .unwrap()
  }

  pub async fn gen_postgre_pool() -> Pool {
    let docker_api = gen_docker_client();
    let ip_addr = store::get_store_ip_addr(&docker_api).await.unwrap();

    store::create_pool(ip_addr).await
  }

  pub async fn generate_server(config: Config) -> test::TestServer {
    before();
    // Build a test daemon config
    let daemon_config = DaemonConfig {
      state_dir: String::from("/var/lib/nanocl"),
      ..Default::default()
    };
    let event_emitter = EventEmitter::new();
    // Create docker_api
    let docker_api = gen_docker_client();
    // Create postgres pool
    let pool = gen_postgre_pool().await;
    // Create test server
    test::server(move || {
      App::new()
        .state(daemon_config.clone())
        .state(pool.clone())
        .state(docker_api.clone())
        .state(event_emitter.clone())
        .configure(config)
    })
  }
}
