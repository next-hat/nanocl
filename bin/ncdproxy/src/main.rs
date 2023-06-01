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

  logger::enable_logger("ncdproxy");
  log::info!("ncdproxy v{}", version::VERSION);

  let nginx = match subsystem::init(&cli) {
    Err(err) => {
      err.exit();
    }
    Ok(nginx) => nginx,
  };

  let server = server::generate(&nginx)?;

  server.await?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::utils::tests;

  #[ntex::test]
  async fn test_scenario() {
    tests::before();
    let res =
      tests::exec_nanocl("state apply -ys ../tests/test-deploy.yml").await;

    assert!(res.is_ok());

    let res = tests::exec_nanocl("state rm -ys ../tests/test-deploy.yml").await;

    assert!(res.is_ok());
  }
}
