use crate::client::Nanocld;
use crate::models::ApplyArgs;

use crate::yml;

use super::errors::CliError;

pub async fn exec_apply(
  client: &Nanocld,
  args: &ApplyArgs,
) -> Result<(), CliError> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(&args.file_path);
  yml::config::apply(file_path, client).await?;
  Ok(())
}
