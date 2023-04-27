use std::path::Path;

use tokio_util::codec;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};

use nanocl_utils::io_error::{IoResult, FromIo};

use nanocld_client::NanocldClient;
use nanocld_client::stubs::vm_image::VmImageCloneStream;

use crate::utils::math::calculate_percentage;

use crate::models::{
  VmImageArgs, VmImageCreateOpts, VmImageCommands, VmImageRow,
  VmImageResizeOpts,
};
use crate::utils::print::print_table;

async fn exec_vm_image_create(
  client: &NanocldClient,
  options: &VmImageCreateOpts,
) -> IoResult<()> {
  let file_path = options.file_path.clone();

  let fp = Path::new(&file_path)
    .canonicalize()
    .map_err(|err| err.map_err_context(|| file_path.to_string()))?;

  let file = tokio::fs::File::open(&fp)
    .await
    .map_err(|err| err.map_err_context(|| file_path.to_string()))?;

  // Get file size
  let file_size = file
    .metadata()
    .await
    .map_err(|err| err.map_err_context(|| file_path.to_string()))?
    .len();
  let mut sent: u64 = 0;
  let pg = ProgressBar::new(100);
  let style = ProgressStyle::with_template(
    "[{elapsed_precise}] [{bar:20.cyan/blue}] {pos:>7}% {msg}",
  )
  .unwrap()
  .progress_chars("=> ");
  pg.set_style(style);

  let byte_stream =
    codec::FramedRead::new(file, codec::BytesCodec::new()).map(move |r| {
      let r = r?;
      sent += r.len() as u64;
      let percent = calculate_percentage(sent, file_size);
      pg.set_position(percent);
      let bytes = ntex::util::Bytes::from_iter(r.freeze().to_vec());
      Ok::<ntex::util::Bytes, std::io::Error>(bytes)
    });

  client.import_vm_image(&options.name, byte_stream).await?;

  Ok(())
}

async fn exec_vm_image_ls(client: &NanocldClient) -> IoResult<()> {
  let items = client.list_vm_image().await?;
  let rows = items
    .into_iter()
    .map(VmImageRow::from)
    .collect::<Vec<VmImageRow>>();

  print_table(rows);
  Ok(())
}

async fn exec_vm_image_rm(
  client: &NanocldClient,
  names: &[String],
) -> IoResult<()> {
  for name in names {
    client.delete_vm_image(name).await?;
  }
  Ok(())
}

async fn exec_vm_image_clone(
  client: &NanocldClient,
  name: &str,
  clone_name: &str,
) -> IoResult<()> {
  let mut stream = client.clone_vm_image(name, clone_name).await?;
  let pg = ProgressBar::new(100);
  let style = ProgressStyle::with_template(
    "[{elapsed_precise}] [{bar:20.cyan/blue}] {pos:>7}% {msg}",
  )
  .unwrap()
  .progress_chars("=> ");
  pg.set_style(style);
  while let Some(item) = stream.next().await {
    let item = item?;
    match item {
      VmImageCloneStream::Progress(progress) => {
        pg.set_position((progress * 100.0) as u64 / 100);
      }
      VmImageCloneStream::Done(_) => {
        pg.finish_and_clear();
      }
    }
  }
  Ok(())
}

async fn exec_vm_resize(
  client: &NanocldClient,
  options: &VmImageResizeOpts,
) -> IoResult<()> {
  let payload = options.clone().into();
  client.resize_vm_image(&options.name, &payload).await?;
  Ok(())
}

pub async fn exec_vm_image(
  client: &NanocldClient,
  args: &VmImageArgs,
) -> IoResult<()> {
  match &args.commands {
    VmImageCommands::Create(options) => {
      exec_vm_image_create(client, options).await
    }
    VmImageCommands::List => exec_vm_image_ls(client).await,
    VmImageCommands::Remove { names } => exec_vm_image_rm(client, names).await,
    VmImageCommands::Clone { name, clone_name } => {
      exec_vm_image_clone(client, name, clone_name).await
    }
    VmImageCommands::Resize(opts) => exec_vm_resize(client, opts).await,
  }
}
