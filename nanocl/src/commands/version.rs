use nanocl_client::NanoclClient;

use crate::version;

use crate::error::CliError;

pub async fn exec_version(client: &NanoclClient) -> Result<(), CliError> {
  println!("=== [nanocli] ===");
  version::print_version();
  println!("=== [nanocld] ===");
  let daemon_version = client.get_version().await?;
  println!(
    "Arch: {}\nVersion: {}\nCommit ID: {}",
    daemon_version.arch, daemon_version.version, daemon_version.commit_id
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn test_exec_version() {
    let client = NanoclClient::connect_with_unix_default().await;
    exec_version(&client).await.unwrap();
  }
}
