use futures::StreamExt;
use nanocld_client::NanoclClient;

use crate::error::CliError;

pub async fn exec_events(client: &NanoclClient) -> Result<(), CliError> {
  let mut stream = client.watch_events().await?;

  while let Some(event) = stream.next().await {
    let event = serde_yaml::to_string(&event)?;
    println!("{}", &event);
  }

  Ok(())
}
