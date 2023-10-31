use clap::Parser;

use nanocl_utils::logger;

mod cli;
mod nginx;
mod utils;
mod server;
mod version;
mod services;
mod subsystem;

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let cli = cli::Cli::parse();
  logger::enable_logger("ncproxy");
  log::info!(
    "ncproxy_{}_{}_v{}:{}",
    version::ARCH,
    version::CHANNEL,
    version::VERSION,
    version::COMMIT_ID
  );
  let nginx = match subsystem::init(&cli).await {
    Err(err) => {
      log::error!("{err}");
      err.exit();
    }
    Ok(nginx) => nginx,
  };
  let server = server::generate(&nginx)?;
  server.await?;
  Ok(())
}
