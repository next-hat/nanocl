use std::path::Path;

use tokio_util::codec;
use futures::StreamExt;
use nanocld_client::NanocldClient;

use crate::error::CliError;
use crate::models::{VmImageArgs, VmImageCreateBaseOpts, VmImageCommands};

async fn exec_vm_image_base_create(
  client: &NanocldClient,
  options: &VmImageCreateBaseOpts,
) -> Result<(), CliError> {
  let file_path = &options.file_path;

  let fp =
    Path::new(file_path)
      .canonicalize()
      .map_err(|err| CliError::Custom {
        msg: format!("Unable to resolve path {file_path}: {err}"),
      })?;

  let file =
    tokio::fs::File::open(&fp)
      .await
      .map_err(|err| CliError::Custom {
        msg: format!("Unable to open file at {file_path}: {err}"),
      })?;

  let byte_stream =
    codec::FramedRead::new(file, codec::BytesCodec::new()).map(|r| {
      let bytes = ntex::util::Bytes::from_iter(r?.to_vec());
      Ok::<ntex::util::Bytes, std::io::Error>(bytes)
    });

  let mut stream = client
    .create_vm_image_base_from_tar(&options.name, byte_stream)
    .await?;

  while let Some(status) = stream.next().await {
    let status = match status {
      Err(err) => {
        eprintln!("Error while importing image: {err}");
        break;
      }
      Ok(status) => status,
    };
    println!("{status:#?}");
  }

  Ok(())
}

pub async fn exec_vm_image(
  client: &NanocldClient,
  args: &VmImageArgs,
) -> Result<(), CliError> {
  match &args.commands {
    VmImageCommands::CreateBase(options) => {
      exec_vm_image_base_create(client, options).await
    }
  }
}
