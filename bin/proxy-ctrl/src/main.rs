mod cli;
mod error;
mod nginx;
mod utils;
mod service;

use clap::Parser;

use ntex::web::{HttpServer, App};
use nanocld_client::NanocldClient;

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let cli = cli::Cli::parse();

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

  let nginx = nginx::new(&cli.conf_dir.unwrap_or("/etc/nginx".into()));
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

  let mut server = HttpServer::new(move || {
    App::new()
      .state(nginx.clone())
      .configure(service::ntex_config)
  });

  server = server.bind_uds("/run/nanocl/proxy.sock")?;

  server.run().await?;
  Ok(())
}
