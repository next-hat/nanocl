use ntex::web;
use ntex_cors::Cors;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};

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
  let daemon_state_ptr = daemon_state.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      // bind config state
      .state(daemon_state_ptr.clone())
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
  let config = daemon_state.config.clone();
  let mut count = 0;
  let hosts = config.hosts.clone();
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
      if let Some(ssl) = config.ssl.clone() {
        log::debug!("server::gen: {addr}: with ssl");
        let cert = ssl.cert.clone().unwrap();
        let cert_key = ssl.cert_key.clone().unwrap();
        let cert_ca = ssl.cert_ca.clone().unwrap();
        server = match server.bind_openssl(&addr, {
          let mut builder =
            SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
          builder
            .set_private_key_file(cert_key, SslFiletype::PEM)
            .unwrap();
          builder.set_certificate_chain_file(cert).unwrap();
          builder.set_ca_file(cert_ca).expect("Failed to set ca file");
          builder.set_verify(
            SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT,
          );
          builder
        }) {
          Err(err) => {
            log::error!("server::gen: {addr}: {err}");
            return Err(err);
          }
          Ok(server) => server,
        };
      } else {
        server = match server.bind(&addr) {
          Err(err) => {
            log::error!("server::gen: {addr}: {err}");
            return Err(err);
          }
          Ok(server) => server,
        };
      }
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
  // get max cpu
  let num = num_cpus::get();
  let workers = if num < 2 { 1 } else { num / 2 };
  server = server.workers(workers);
  Ok(server.run())
}

/// Server init test
#[cfg(test)]
mod tests {
  use clap::Parser;
  use nanocl_stubs::system::BinaryInfo;
  use ntex::http::{client::Connector, StatusCode};
  use openssl::ssl::SslConnector;

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

  #[ntex::test]
  async fn ssl_valid_client() {
    let args = init_test_config(vec![
      "nanocl",
      "-H",
      "tcp://0.0.0.0:6443",
      "--cert",
      "../../tests/server.crt",
      "--cert-key",
      "../../tests/server.key",
      "--cert-ca",
      "../../tests/ca.crt",
    ]);
    assert_config_ok(args).await;
    // Configure SSL/TLS settings
    let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
    builder.set_verify(SslVerifyMode::NONE);
    builder
      .set_certificate_file("../../tests/client.crt", SslFiletype::PEM)
      .unwrap();
    builder
      .set_private_key_file("../../tests/client.key", SslFiletype::PEM)
      .unwrap();
    let client = ntex::http::client::Client::build()
      .connector(Connector::default().openssl(builder.build()).finish())
      .finish();
    let mut res = client
      .get("https://0.0.0.0:6443/v0.13/version")
      .send()
      .await
      .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let version = res.json::<BinaryInfo>().await.unwrap();
    assert_eq!(version.version, vars::VERSION);
  }

  #[ntex::test]
  async fn ssl_wrong_client() {
    let args = init_test_config(vec![
      "nanocl",
      "-H",
      "tcp://0.0.0.0:4443",
      "--cert",
      "../../tests/server.crt",
      "--cert-key",
      "../../tests/server.key",
      "--cert-ca",
      "../../tests/ca.crt",
    ]);
    assert_config_ok(args).await;
    // Configure SSL/TLS settings
    let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
    builder.set_verify(SslVerifyMode::NONE);
    let client = ntex::http::client::Client::build()
      .connector(Connector::default().openssl(builder.build()).finish())
      .finish();
    let res = client
      .get("https://0.0.0.0:4443/v0.13/version")
      .send()
      .await;
    assert!(res.is_err());
  }
}
