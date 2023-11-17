use nanocl_error::io::IoResult;
use nanocld_client::NanocldClient;

use crate::{version, config::CliConfig};

/// ## Print version
///
/// Print version of nanocli and nanocld
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
///
async fn print_version(client: &NanocldClient) -> IoResult<()> {
  println!("=== [nanocli] ===");
  version::print_version();
  let daemon_version = client.get_version().await?;
  println!("=== [nanocld] ===");
  println!(
    "Arch: {}\nChannel: {}\nVersion: {}\nCommit ID: {}",
    daemon_version.arch,
    daemon_version.channel,
    daemon_version.version,
    daemon_version.commit_id
  );
  Ok(())
}

/// ## Exec version
///
/// Function that execute when running `nanocl version`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
///
pub async fn exec_version(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  print_version(client).await?;
  Ok(())
}
