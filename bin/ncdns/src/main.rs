use clap::Parser;

use nanocl_utils::logger;
use nanocl_error::io::IoResult;

mod cli;
mod utils;
mod event;
mod server;
mod version;
mod dnsmasq;
mod services;

use nanocld_client::NanocldClient;

use cli::Cli;
use dnsmasq::Dnsmasq;

async fn run(cli: &Cli) -> IoResult<()> {
  logger::enable_logger("ncdns");
  log::info!(
    "ncdns_{}_{}_v{}:{}",
    version::ARCH,
    version::CHANNEL,
    version::VERSION,
    version::COMMIT_ID
  );
  // Spawn a new thread to listen events from nanocld
  let conf_dir = cli.conf_dir.to_owned().unwrap_or("/etc".into());
  let dnsmasq = Dnsmasq::new(&conf_dir).with_dns(cli.dns.clone()).ensure()?;
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
  }
  event::spawn(&client);
  let server = server::generate(&cli.host, &dnsmasq, &client)?;
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
  use nanocl_error::io::IoResult;

  #[ntex::test]
  async fn run_wrong_host() -> IoResult<()> {
    let cli = Cli::parse_from([
      "ncdns",
      "--host",
      "wrong://dsadsa",
      "--conf-dir",
      "/tmp/ncdns",
      "--dns",
      "1.1.1.1",
    ]);
    let server = run(&cli).await;
    assert!(server.is_err());
    Ok(())
  }
}
