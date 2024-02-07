use clap::Parser;

use nanocl_utils::logger;
use nanocl_error::io::IoResult;

mod cli;
mod utils;
mod event;
mod server;
mod vars;
mod dnsmasq;
mod services;

use nanocld_client::NanocldClient;

use cli::Cli;
use dnsmasq::Dnsmasq;

async fn run(cli: &Cli) -> IoResult<()> {
  // Spawn a new thread to listen events from nanocld
  let dnsmasq = Dnsmasq::new(&cli.state_dir)
    .with_dns(cli.dns.clone())
    .ensure()?;
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    use nanocld_client::ConnectOpts;
    client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    });
  }
  event::spawn(&client);
  let server = server::gen(&cli.host, &dnsmasq, &client)?;
  server.await?;
  Ok(())
}

#[ntex::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  logger::enable_logger("ncdns");
  log::info!(
    "ncdns_{}_v{}-{}:{}",
    vars::ARCH,
    vars::VERSION,
    vars::CHANNEL,
    vars::COMMIT_ID
  );
  let cli = Cli::parse();
  if let Err(err) = run(&cli).await {
    err.print_and_exit();
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
      "wrong://dadas",
      "--state-dir",
      "/tmp/ncdns",
      "--dns",
      "1.1.1.1",
    ]);
    let server = run(&cli).await;
    assert!(server.is_err());
    Ok(())
  }
}
