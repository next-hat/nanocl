use clap::Parser;

use nanocl_utils::logger;
use nanocl_utils::io_error::IoResult;

mod cli;
mod utils;
mod server;
mod version;
mod dnsmasq;
mod services;

use cli::Cli;
use dnsmasq::Dnsmasq;

async fn run(cli: &Cli) -> IoResult<()> {
  logger::enable_logger("ncddns");
  log::info!("ncddns v{}", env!("CARGO_PKG_VERSION"));

  let conf_dir = cli.conf_dir.to_owned().unwrap_or("/etc".into());
  let dnsmasq = Dnsmasq::new(&conf_dir).with_dns(cli.dns.clone());
  if let Err(err) = dnsmasq.ensure() {
    err.exit();
  }

  let server = server::generate(&cli.host, &dnsmasq)?;
  server.await?;

  Ok(())
}

#[ntex::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let cli = Cli::parse();

  if let Err(err) = run(&cli).await {
    err.exit();
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  // use ntex::rt;
  use nanocl_utils::io_error::IoResult;

  #[ntex::test]
  async fn run_wrong_host() -> IoResult<()> {
    let cli = Cli::parse_from([
      "ncddns",
      "--host",
      "wrong://dsadsa",
      "--conf-dir",
      "/tmp/ncddns",
    ]);
    let server = run(&cli).await;
    assert!(server.is_err());
    Ok(())
  }
}
