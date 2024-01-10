use ntex::web;
use ntex_cors::Cors;

use nanocl_utils::ntex::middlewares;

use crate::{vars, services, models::SystemState};

/// This function will generate the HTTP server with the given configuration.
/// It will also bind the server to the given address.
/// The server will be returned.
/// NOTE: In development we bind the address to [http://0.0.0.0:8585](http://0.0.0.0:8585)
///       with an explorer on [http://0.0.0.0:8585/explorer/](http://0.0.0.0:8585/explorer/)
pub async fn gen(
  daemon_state: SystemState,
) -> std::io::Result<ntex::server::Server> {
  log::info!("server::gen: start");
  let hosts = daemon_state.config.hosts.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      // bind config state
      .state(daemon_state.clone())
      .state(
        web::types::PayloadConfig::new(20_000_000_000), // <- limit size of the payload
      )
      .wrap(Cors::new().finish())
      .wrap(middlewares::Versioning::new(vars::VERSION).finish())
      .wrap(middlewares::SerializeError)
      // Default logger middleware
      .wrap(web::middleware::Logger::default())
      // Set Json body max size
      .state(web::types::JsonConfig::default().limit(20_000_000))
      .configure(services::ntex_config)
      .default_service(web::route().to(services::unhandled))
  });
  let mut count = 0;
  let len = hosts.len();
  while count < len {
    let host = &hosts[count];
    if host.starts_with("unix://") {
      let addr = host.replace("unix://", "");
      server = match server.bind_uds(&addr) {
        Err(err) => {
          log::error!("server::gen: {addr}: {err}");
          return Err(err);
        }
        Ok(server) => server,
      };
    } else if host.starts_with("tcp://") {
      let addr = host.replace("tcp://", "");
      server = match server.bind(&addr) {
        Err(err) => {
          log::error!("server::gen: {addr}: {err}");
          return Err(err);
        }
        Ok(server) => server,
      };
    } else {
      log::error!(
        "server::gen: {} invalid protocol [tcp:// | unix://] allowed",
        host
      );
      return Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "invalid protocol [tcp:// | unix://] allowed",
      ));
    }
    log::info!("server::gen: {host}");
    count += 1;
  }
  #[cfg(feature = "dev")]
  {
    server = server.bind("0.0.0.0:8585")?;
    log::debug!("server::gen: dev mode http://0.0.0.0:8585");
    log::debug!(
      "server::gen: swagger available at http://0.0.0.0:8585/explorer/"
    );
  }
  log::info!("server::gen: ready");
  Ok(server.run())
}

/// Server init test
#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  use crate::{config, cli::Cli, utils::tests::*};

  fn init_test_config(cmd: Vec<&str>) -> Cli {
    before();
    let mut cmd = cmd.clone();
    let home = std::env::var("HOME").expect("Failed to get home dir");
    let state_dir = format!("{home}/.nanocl_dev/state");
    cmd.push("--state-dir");
    cmd.push(&state_dir);
    Cli::parse_from(cmd)
  }

  async fn test_config(
    args: Cli,
  ) -> Result<ntex::server::Server, std::io::Error> {
    let daemon_conf = config::init(&args).expect("Expect config to be valid");
    let daemon_state = SystemState::new(&daemon_conf)
      .await
      .expect("Init daemon state to be ok");
    gen(daemon_state).await
  }

  async fn assert_config_ok(args: Cli) {
    assert!(
      test_config(args.clone()).await.is_ok(),
      "Expected succcess for {:#?}",
      args
    );
  }

  async fn assert_config_err(args: Cli) {
    assert!(
      test_config(args.clone()).await.is_err(),
      "Expected error for {:#?}",
      args
    );
  }

  /// Test to create a server on unix socket
  #[ntex::test]
  async fn server_on_tmp_unix_socket() {
    let args =
      init_test_config(vec!["nanocl", "-H", "unix:///tmp/nanocl_test.sock"]);
    assert_config_ok(args).await;
  }

  /// Test to create a server on tcp socket
  #[ntex::test]
  async fn server_on_tcp_socket() {
    let args = init_test_config(vec!["nanocl", "-H", "tcp://127.0.0.1:9999"]);
    assert_config_ok(args).await;
  }

  ///  Test to create 2 server on same tcp socket
  /// Expect the 2nd one to fail
  #[ntex::test]
  async fn server_on_same_tcp_socket() {
    let args = init_test_config(vec!["nanocl", "-H", "tcp://127.0.0.1:9888"]);
    assert_config_ok(args.clone()).await;
    assert_config_err(args).await;
  }

  /// Test with invalid host uri
  /// Expect the server to fail
  #[ntex::test]
  async fn server_on_invalid_host() {
    let args = init_test_config(vec!["nanocl", "-H", "not_valid"]);
    assert_config_err(args).await;
  }
}
