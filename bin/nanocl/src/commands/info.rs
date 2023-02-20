use nanocld_client::NanocldClient;

use crate::error::CliError;

pub async fn exec_info(client: &NanocldClient) -> Result<(), CliError> {
  let info = client.info().await?;
  let info = serde_yaml::to_string(&info)?;

  println!("{info}");
  Ok(())
}
