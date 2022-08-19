use futures::{TryStreamExt, StreamExt};
use serde::{Serialize, Deserialize};
use clap::Parser;

use super::client::Nanocld;
use super::error::{NanocldError, is_api_error};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct VmImageImportPayload {
  pub(crate) name: String,
  pub(crate) url: String,
}

impl Nanocld {
  pub async fn vm_image_import(
    &self,
    payload: &VmImageImportPayload,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(String::from("/virtual_machine_images/import"))
      .send_json(payload)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let mut stream = res.into_stream();
    while let Some(chunk) = stream.next().await {
      let data = chunk.unwrap();
      println!("{:#?}", &data);
    }
    Ok(())
  }
}
