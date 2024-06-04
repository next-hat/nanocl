/// # ncproxy
///
/// The program in charge of managing writing proxy configuration.
/// It's based on nginx and use the nanocld api to get the configuration wanted by the user.
///
/// It work by doing 4 main tasks:ncproxy is
/// - Create a new rule in nginx when a resource `ncproxy.io/rule` is created
/// - Delete a new rule in nginx when a resource `ncproxy.io/rule` is deleted
/// - Watch nanocld events for resource, cargo and vm change to update proxy rules accordingly
/// - Send a reload task to nginx when a rule is created, deleted or updated
///
use clap::Parser;

use nanocl_utils::logger;

mod cli;
mod utils;
mod models;
mod vars;
mod services;
mod subsystem;

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let cli = cli::Cli::parse();
  #[cfg(feature = "dev")]
  {
    std::env::set_var("LOG_LEVEL", "ncproxy=trace");
  }
  logger::enable_logger("ncproxy");
  log::info!(
    "ncproxy_{}_v{}-{}:{}",
    vars::ARCH,
    vars::VERSION,
    vars::CHANNEL,
    vars::COMMIT_ID
  );
  let state = match subsystem::init(&cli).await {
    Err(err) => err.print_and_exit(),
    Ok(state) => state,
  };
  match utils::server::gen(&state) {
    Err(err) => err.print_and_exit(),
    Ok(srv) => srv.await,
  }
}
