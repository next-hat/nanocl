use clap::Parser;

use nanocl_utils::logger;

mod cli;
mod utils;
mod server;
mod version;
mod dnsmasq;
mod services;

use cli::Cli;
use dnsmasq::Dnsmasq;

#[ntex::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let cli = Cli::parse();

  logger::enable_logger("ncddns");
  log::info!("ncddns v{}", env!("CARGO_PKG_VERSION"));

  let conf_dir = cli.conf_dir.to_owned().unwrap_or("/etc".into());
  let dnsmasq = Dnsmasq::new(&conf_dir).with_dns(cli.dns);
  if let Err(err) = dnsmasq.ensure() {
    err.exit();
  }

  let server = match server::generate(&dnsmasq) {
    Err(err) => err.exit(),
    Ok(server) => server,
  };

  server.await?;

  Ok(())
}
