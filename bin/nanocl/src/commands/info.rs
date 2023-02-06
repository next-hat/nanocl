use nanocld_client::NanoclClient;

use crate::error::CliError;

pub async fn exec_info(client: &NanoclClient) -> Result<(), CliError> {
  let info = client.info().await?;
  let info = serde_yaml::to_string(&info)?;

  println!("{info}");
  Ok(())
}
