use clap::Parser;

use nanocl_error::io::IoResult;
use nanocl_utils::logger;

mod cli;
mod dnsmasq;
mod event;
mod server;
mod services;
mod utils;
mod vars;

use nanocld_client::NanocldClient;

use cli::Cli;
use dnsmasq::Dnsmasq;

const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

async fn run(cli: &Cli) -> IoResult<()> {
  // Spawn a new thread to listen events from nanocld
  let dnsmasq = Dnsmasq::new(&cli.state_dir)
    .with_dns(cli.dns.clone())
    .ensure()?;
  dnsmasq.start().await?;
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    use nanocld_client::ConnectOpts;
    client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })?;
  }
  loop {
    match event::ensure_self_config(&client).await {
      Ok(()) => break,
      Err(err) => {
        log::warn!("run: {err}");
        ntex::time::sleep(RETRY_DELAY).await;
      }
    }
  }
  event::spawn(&client);
  let server = server::generate(&cli.host, &dnsmasq, &client)?;
  dnsmasq.spawn_monitor();
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
