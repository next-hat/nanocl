use ntex::web;

use crate::services;
use crate::models::DaemonState;

pub async fn start(
  daemon_state: DaemonState,
) -> std::io::Result<ntex::server::Server> {
  log::info!("Preparing server");
  let hosts = daemon_state.config.hosts.to_owned();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      // bind config state
      .state(daemon_state.config.clone())
      // bind postgre pool to state
      .state(daemon_state.pool.clone())
      // bind docker api
      .state(daemon_state.docker_api.clone())
      // Default logger middleware
      .wrap(web::middleware::Logger::default())
      // Set Json body max size
      .state(web::types::JsonConfig::default().limit(4096))
      // configure system service
      .configure(services::system::ntex_config)
      // configure namespace service
      .configure(services::namespace::ntex_config)
      // configure cargo image service
      .configure(services::cargo_image::ntex_config)
      // configure cargo service
      .configure(services::cargo::ntex_config)
  });
  let mut count = 0;
  let len = hosts.len();
  while count < len {
    let host = &hosts[count];
    if host.starts_with("unix://") {
      let addr = host.replace("unix://", "");
      server = match server.bind_uds(&addr) {
        Err(err) => {
          log::error!("Error binding to unix socket: {}", err);
          return Err(err);
        }
        Ok(server) => server,
      };
      log::info!("Listening on {}", &host);
    } else if host.starts_with("tcp://") {
      let addr = host.replace("tcp://", "");
      server = match server.bind(&addr) {
        Err(err) => {
          log::error!("Error binding to tcp socket: {}", err);
          return Err(err);
        }
        Ok(server) => server,
      };
      log::info!("Listening on {}", &host);
    } else {
      log::error!(
        "Error {} is not valid use tcp:// or unix:// as protocol",
        host
      );
      return Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Invalid protocol use tcp:// or unix://",
      ));
    }
    count += 1;
  }
  #[cfg(feature = "dev")]
  {
    server = server.bind("0.0.0.0:29875")?;
    log::info!("Dev mode enabled, listening on http://localhost:29875");
  };
  // server = server.bind_rustls("0.0.0.0:8443", server_config)?;
  log::info!("Server ready");
  Ok(server.run())
}

/// Server init test
#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  use crate::cli::Cli;
  use crate::state;
  use crate::utils::tests::*;

  /// Test to create a server on unix socket
  #[ntex::test]
  async fn test_server_on_tmp_unix_socket() -> TestRet {
    before();
    let args =
      Cli::parse_from(vec!["nanocl", "-H", "unix:///tmp/nanocl_test.sock"]);
    let daemon_state = state::init(&args).await?;
    let server = start(daemon_state).await;
    assert!(server.is_ok(), "Expect server to be ready to run");
    Ok(())
  }

  /// Test to create a server on tcp socket
  #[ntex::test]
  async fn test_server_on_tcp_socket() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "tcp://127.0.0.1:9999"]);
    let daemon_state = state::init(&args).await?;
    let server = start(daemon_state).await;
    assert!(server.is_ok(), "Expect server to be ready to run");
    Ok(())
  }

  ///  Test to create 2 server on same tcp socket
  /// Expect the 2nd one to fail
  #[ntex::test]
  async fn test_server_on_same_tcp_socket() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "tcp://127.0.0.1:9888"]);
    let daemon_state = state::init(&args).await?;
    let server = start(daemon_state).await;
    assert!(server.is_ok(), "Expect server to be ready to run");
    let daemon_state = state::init(&args).await?;
    let server2 = start(daemon_state).await;
    assert!(server2.is_err(), "Expect server to fail to run");
    Ok(())
  }

  /// Test to create a server on unix socket where path is not valid
  /// Expect the server to fail
  #[ntex::test]
  async fn test_server_on_invalid_unix_socket() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "unix:///root/test.sock"]);
    let daemon_state = state::init(&args).await?;
    let server = start(daemon_state).await;
    assert!(server.is_err(), "Expect server to fail to run");
    Ok(())
  }

  /// Test with invalid host uri
  /// Expect the server to fail
  #[ntex::test]
  async fn test_server_on_invalid_host() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "not_valid"]);
    let daemon_state = state::init(&args).await?;
    let server = start(daemon_state).await;
    assert!(server.is_err(), "Expect server to fail to run");
    Ok(())
  }
}
