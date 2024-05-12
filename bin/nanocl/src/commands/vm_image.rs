use std::path::Path;

use tokio_util::codec;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};

use nanocl_error::io::{IoResult, FromIo};
use nanocld_client::{
  NanocldClient,
  stubs::vm_image::{VmImage, VmImageCloneStream},
};

use crate::{
  utils,
  models::{
    GenericDefaultOpts, VmImageArg, VmImageCommand, VmImageCreateOpts,
    VmImageResizeOpts, VmImageRow,
  },
};

use super::{GenericCommand, GenericCommandLs, GenericCommandRm};

impl GenericCommand for VmImageArg {
  fn object_name() -> &'static str {
    "vms/images"
  }
}

impl GenericCommandLs for VmImageArg {
  type Item = VmImageRow;
  type Args = VmImageArg;
  type ApiItem = VmImage;

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for VmImageArg {}

/// Function that execute when running `nanocl vm image create`
async fn exec_vm_image_create(
  client: &NanocldClient,
  options: &VmImageCreateOpts,
) -> IoResult<()> {
  let file_path = options.file_path.clone();
  let fp = Path::new(&file_path)
    .canonicalize()
    .map_err(|err| err.map_err_context(|| file_path.to_owned()))?;
  let file = tokio::fs::File::open(&fp)
    .await
    .map_err(|err| err.map_err_context(|| file_path.to_owned()))?;
  // Get file size
  let file_size = file
    .metadata()
    .await
    .map_err(|err| err.map_err_context(|| file_path.to_owned()))?
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
      let percent = utils::math::calculate_percentage(sent, file_size);
      pg.set_position(percent);
      let bytes = ntex::util::Bytes::from_iter(r.freeze().to_vec());
      Ok::<ntex::util::Bytes, std::io::Error>(bytes)
    });
  client.import_vm_image(&options.name, byte_stream).await?;
  Ok(())
}

/// Function that execute when running `nanocl vm image clone`
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

/// Function that execute when running `nanocl vm resize`
async fn exec_vm_resize(
  client: &NanocldClient,
  options: &VmImageResizeOpts,
) -> IoResult<()> {
  let payload = options.clone().into();
  client.resize_vm_image(&options.name, &payload).await?;
  Ok(())
}

/// Function that execute when running `nanocl vm image`
pub async fn exec_vm_image(
  client: &NanocldClient,
  args: &VmImageArg,
) -> IoResult<()> {
  match &args.command {
    VmImageCommand::Create(options) => {
      exec_vm_image_create(client, options).await
    }
    VmImageCommand::List(opts) => VmImageArg::exec_ls(client, args, opts).await,
    VmImageCommand::Remove(opts) => {
      VmImageArg::exec_rm(client, opts, None).await
    }
    VmImageCommand::Clone { name, clone_name } => {
      exec_vm_image_clone(client, name, clone_name).await
    }
    VmImageCommand::Resize(opts) => exec_vm_resize(client, opts).await,
  }
}
