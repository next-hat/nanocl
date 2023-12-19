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
    "ncproxy_{}_v{}-{}:{}",
    version::ARCH,
    version::VERSION,
    version::CHANNEL,
    version::COMMIT_ID
  );
  let (nginx, client) = match subsystem::init(&cli).await {
    Err(err) => {
      err.print_and_exit();
    }
    Ok(nginx) => nginx,
  };
  let server = server::gen(&nginx, &client)?;
  server.await?;
  Ok(())
}
