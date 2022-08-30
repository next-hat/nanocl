use crate::client::Nanocld;
use crate::models::RevertArgs;

use crate::yml;
use super::errors::CliError;

pub async fn exec_revert(
  client: &Nanocld,
  args: &RevertArgs,
) -> Result<(), CliError> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(&args.file_path);
  yml::config::revert(file_path, client).await?;
  Ok(())
}
