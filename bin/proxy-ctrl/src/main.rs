mod cli;
mod error;
mod nginx;
mod utils;
mod service;

use clap::Parser;

use ntex::web::{App, HttpServer};
use nanocld_client::NanocldClient;

async fn boot(cli: &cli::Cli) -> nginx::Nginx {
  if std::env::var("LOG_LEVEL").is_err() {
    std::env::set_var("LOG_LEVEL", "nanocl-ctrl-proxy=info,warn,error,debug");
  }
  let is_test = std::env::var("TEST").is_ok();
  env_logger::Builder::new()
    .parse_env("LOG_LEVEL")
    .format_target(false)
    .is_test(is_test)
    .init();

  log::info!("nanocl-ctrl-proxy v{}", env!("CARGO_PKG_VERSION"));

  let nginx = nginx::new(&cli.conf_dir.clone().unwrap_or("/etc/nginx".into()));
  let client = NanocldClient::connect_with_unix_default();

  if let Err(err) = nginx.ensure() {
    err.exit();
  }

  if let Err(err) = nginx.write_default_conf() {
    err.exit();
  }

  if let Err(err) = utils::sync_resources(&client, &nginx).await {
    err.exit();
  }

  nginx
}

fn main() -> std::io::Result<()> {
  ntex::rt::System::new(stringify!("run")).block_on(async move {
    let cli = cli::Cli::parse();

    let nginx = boot(&cli).await;

    let mut server = HttpServer::new(move || {
      App::new()
        .state(nginx.clone())
        .configure(service::ntex_config)
    });

    server = server.bind_uds("/run/nanocl/proxy.sock")?;

    server.run().await?;
    Ok::<_, std::io::Error>(())
  })?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use crate::utils::tests;

  #[ntex::test]
  async fn boot() {
    let cli =
      super::cli::Cli::parse_from(["proxy-ctrl", "--conf-dir", "/tmp/nginx"]);

    super::boot(&cli).await;
  }

  #[ntex::test]
  async fn test_scenario() {
    let res =
      tests::exec_nanocl("nanocl state apply -yf ../tests/test-deploy.yml")
        .await;

    assert!(res.is_ok());

    let res =
      tests::exec_nanocl("nanocl state revert -yf ../tests/test-deploy.yml")
        .await;

    assert!(res.is_ok());
  }
}
