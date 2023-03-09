use nanocld_client::NanocldClient;

use crate::version;

use crate::error::CliError;

pub async fn exec_version(client: &NanocldClient) -> Result<(), CliError> {
  print_version(client).await?;
  Ok(())
}

async fn print_version(client: &NanocldClient) -> Result<(), CliError> {
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
