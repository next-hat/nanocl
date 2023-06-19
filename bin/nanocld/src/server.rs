use ntex::web;
use ntex_cors::Cors;

use nanocl_utils::ntex::middlewares;

use crate::services;
use crate::models::DaemonState;

/// ## Gen
///
/// This function will generate the HTTP server with the given configuration.
/// It will also bind the server to the given address.
/// The server will be returned.
/// NOTE: In development we bind the address to [http://0.0.0.0:8585](http://0.0.0.0:8585)
///       with an explorer on [http://0.0.0.0:8585/explorer/](http://0.0.0.0:8585/explorer/)
///
/// ## Arguments
///
/// - [daemon_state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](ntex::server::Server) - The HTTP server
///   - [Err](std::io::Error) - Error during the operation
///
pub async fn gen(
  daemon_state: DaemonState,
) -> std::io::Result<ntex::server::Server> {
  log::info!("Preparing server");
  let hosts = daemon_state.config.hosts.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      // bind config state
      .state(daemon_state.clone())
      .state(
        web::types::PayloadConfig::new(20_000_000_000), // <- limit size of the payload
      )
      .wrap(Cors::new().finish())
      .wrap(middlewares::SerializeError)
      // Default logger middleware
      // .wrap(web::middleware::Logger::default())
      // Set Json body max size
      // .state(web::types::JsonConfig::default().limit(4096))
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
          log::error!("Error binding to unix socket {}: {}", &addr, &err);
          return Err(err);
        }
        Ok(server) => server,
      };
      log::info!("Listening on {}", &host);
    } else if host.starts_with("tcp://") {
      let addr = host.replace("tcp://", "");
      server = match server.bind(&addr) {
        Err(err) => {
          log::error!("Error binding to tcp host {}: {}", &addr, &err);
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
    server = server.bind("0.0.0.0:8585")?;
    log::debug!("Running in dev mode, binding to: http://0.0.0.0:8585");
    log::debug!("OpenAPI explorer available at: http://0.0.0.0:8585/explorer/");
  }
  log::info!("Server ready");
  Ok(server.run())
}

/// Server init test
#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  use crate::boot;
  use crate::config;
  use crate::cli::Cli;
  use crate::utils::tests::*;

  /// Test to create a server on unix socket
  #[ntex::test]
  async fn server_on_tmp_unix_socket() -> TestRet {
    before();
    let args =
      Cli::parse_from(vec!["nanocl", "-H", "unix:///tmp/nanocl_test.sock"]);
    let daemon_conf = config::init(&args).expect("Expect config to be valid");
    let daemon_state = boot::init(&daemon_conf)
      .await
      .expect("Init daemon state to be ok");
    let server = gen(daemon_state).await;
    assert!(server.is_ok(), "Expect server to be ready to run");
    Ok(())
  }

  /// Test to create a server on tcp socket
  #[ntex::test]
  async fn server_on_tcp_socket() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "tcp://127.0.0.1:9999"]);
    let daemon_conf = config::init(&args).expect("Expect config to be valid");
    let daemon_state = boot::init(&daemon_conf)
      .await
      .expect("Init daemon state to be ok");
    let server = gen(daemon_state).await;
    assert!(server.is_ok(), "Expect server to be ready to run");
    Ok(())
  }

  ///  Test to create 2 server on same tcp socket
  /// Expect the 2nd one to fail
  #[ntex::test]
  async fn server_on_same_tcp_socket() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "tcp://127.0.0.1:9888"]);
    let daemon_conf = config::init(&args).expect("Expect config to be valid");
    let daemon_state = boot::init(&daemon_conf)
      .await
      .expect("Init daemon state to be ok");
    let server = gen(daemon_state).await;
    assert!(server.is_ok(), "Expect server to be ready to run");
    let daemon_conf = config::init(&args).expect("Expect config to be valid");
    let daemon_state = boot::init(&daemon_conf)
      .await
      .expect("Init daemon state to be ok");
    let server2 = gen(daemon_state).await;
    assert!(server2.is_err(), "Expect server to fail to run");
    Ok(())
  }

  /// Test to create a server on unix socket where path is not valid
  /// Expect the server to fail
  #[ntex::test]
  async fn server_on_invalid_unix_socket() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "unix:///root/test.sock"]);
    let daemon_conf = config::init(&args).expect("Expect config to be valid");
    let daemon_state = boot::init(&daemon_conf)
      .await
      .expect("Init daemon state to be ok");
    let server = gen(daemon_state).await;
    assert!(server.is_err(), "Expect server to fail to run");
    Ok(())
  }

  /// Test with invalid host uri
  /// Expect the server to fail
  #[ntex::test]
  async fn server_on_invalid_host() -> TestRet {
    before();
    let args = Cli::parse_from(vec!["nanocl", "-H", "not_valid"]);
    let daemon_conf = config::init(&args).expect("Expect config to be valid");
    let daemon_state = boot::init(&daemon_conf)
      .await
      .expect("Init daemon state to be ok");
    let server = gen(daemon_state).await;
    assert!(server.is_err(), "Expect server to fail to run");
    Ok(())
  }
}
