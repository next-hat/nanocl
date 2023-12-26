use clap::Parser;

use nanocl_error::io::IoResult;
use nanocl_utils::logger;

mod cli;
mod version;
mod utils;
mod models;
mod services;
mod subsystem;

use cli::Cli;

async fn run(cli: &Cli) -> IoResult<()> {
  let state = subsystem::init(cli).await?;
  let server = utils::server::gen(&cli.host, &state)?;
  server.await?;
  Ok(())
}

#[ntex::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  logger::enable_logger("ncdns");
  log::info!(
    "ncdns_{}_v{}-{}:{}",
    version::ARCH,
    version::VERSION,
    version::CHANNEL,
    version::COMMIT_ID
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
