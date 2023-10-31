use clap::Parser;

use nanocl_utils::logger;
use nanocld_client::NanocldClient;

mod cli;
mod event;
mod version;
mod utils;
mod test;

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let cli = cli::Cli::parse();

  logger::enable_logger("ncertmanager");
  log::info!("ncertmanager_{}_{}", version::ARCH, version::CHANNEL);
  log::info!("v{}:{}", version::VERSION, version::COMMIT_ID);
  let mut client = NanocldClient::connect_with_unix_default();

  event::event_loop(&client).await?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::test::tests;

  #[ntex::test]
  async fn test_scenario() {
    tests::before();
    let res =
      tests::exec_nanocl("state apply -ys ../letsencrypt.Statefile.yml").await;

    assert!(res.is_ok());

    let res = tests::exec_nanocl("state rm -ys ../tests/test-deploy.yml").await;

    assert!(res.is_ok());
  }
}
