use clap::Parser;

use nanocl_error::io::FromIo;
use nanocl_utils::logger;

mod cli;
mod config;
mod models;
mod objects;
mod repositories;
mod schema;
mod services;
mod system;
mod tasks;
mod utils;
mod vars;

/// Provides an api to manage containers and virtual machines across physical hosts
/// There are these advantages :
/// - It's Open source
/// - It's Easy to use
/// - It keep an history of all your containers and virtual machines
fn main() -> std::io::Result<()> {
  // Parse command line arguments
  // Generate openapi specs file in yaml format
  #[cfg(feature = "dev")]
  {
    use crate::services::openapi::ApiDoc;
    use utoipa::OpenApi;
    let api_doc = ApiDoc::openapi();
    std::fs::write(
      "./bin/nanocld/specs/swagger.yaml",
      api_doc.to_yaml().expect("Unable to convert ApiDoc to yaml"),
    )
    .expect("Unable to write swagger.yaml");
  }
  ntex::rt::System::new("main").block_on(async {
    let args = cli::Cli::parse();
    logger::enable_logger(env!("CARGO_PKG_NAME"));
    log::info!(
      "nanocld_{}_v{}-{}:{}",
      vars::ARCH,
      vars::VERSION,
      vars::CHANNEL,
      vars::COMMIT_ID
    );
    // Init config by comparing command line arguments and config file
    let config = match config::init(&args) {
      Err(err) => {
        err.print_and_exit();
      }
      Ok(config) => config,
    };
    // Boot internal dependencies (database, event bus, etc...)
    let daemon_state = match system::init(&config).await {
      Err(err) => {
        err.print_and_exit();
      }
      Ok(daemon_state) => daemon_state,
    };
    // Start http server
    match utils::server::gen(daemon_state).await {
      Err(err) => {
        err.map_err_context(|| "Http server").print_and_exit();
      }
      Ok(server) => {
        // Start http server and wait for shutdown
        // Server should never shutdown unless it's explicitly asked
        if let Err(err) = server.await {
          err.map_err_context(|| "Http server").print_and_exit();
        }
      }
    }
    log::info!("main: shutdown");
  });
  Ok(())
}
