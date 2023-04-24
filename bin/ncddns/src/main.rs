use ntex::web;
use ntex::server::Server;

use nanocl_utils::logger::enable_logger;

mod cli;
mod utils;
mod dnsmasq;
mod service;

use dnsmasq::Dnsmasq;

fn setup_server(dnsmasq: &Dnsmasq) -> std::io::Result<Server> {
  let dnsmasq = dnsmasq.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(dnsmasq.clone())
      .configure(service::ntex_config)
  });

  server = server.bind_uds("/run/nanocl/dns.sock")?;

  Ok(server.run())
}

#[ntex::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let cli = cli::parse();
  enable_logger("ncddns");

  log::info!("nanocl-ncddns v{}", env!("CARGO_PKG_VERSION"));

  let conf_dir = cli.conf_dir.to_owned().unwrap_or("/etc".into());
  let dnsmasq = dnsmasq::new(&conf_dir).with_dns(cli.dns);
  dnsmasq.ensure()?;

  let server = setup_server(&dnsmasq)?;

  server.await?;

  Ok(())
}
