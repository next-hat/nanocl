use std::collections::HashMap;

use clap::Parser;

use nanocl_utils::logger;
use nanocld_client::NanocldClient;

use crate::manager::NCertManager;

mod cli;
mod event;
mod version;
mod utils;
mod manager;
mod test;

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let cli = cli::Cli::parse();
  logger::enable_logger("ncertmanager");
  log::info!("ncertmanager_{}_{}", version::ARCH, version::CHANNEL);
  log::info!("v{}:{}", version::VERSION, version::COMMIT_ID);
  let client = NanocldClient::connect_with_unix_default();

  let mut manager = NCertManager::new(client);

  event::event_loop(&mut manager).await?;

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
