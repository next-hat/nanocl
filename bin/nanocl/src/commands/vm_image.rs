use std::path::Path;

use tokio_util::codec;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};

use nanocl_utils::io_error::{IoResult, FromIo};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::vm_image::VmImageCloneStream;

use crate::utils::print::print_table;
use crate::utils::math::calculate_percentage;

use crate::models::{
  VmImageArg, VmImageCreateOpts, VmImageCommand, VmImageRow, VmImageResizeOpts,
  VmImageListOpts,
};

/// ## Exec vm image create
///
/// Function that execute when running `nanocl vm image create`
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
/// * [options](VmImageCreateOpts) The vm image create options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
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

/// ## Exec vm image ls
///
/// Function that execute when running `nanocl vm image ls`
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
/// * [opts](VmImageListOpts) The vm image list options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_vm_image_ls(
  client: &NanocldClient,
  opts: &VmImageListOpts,
) -> IoResult<()> {
  let items = client.list_vm_image().await?;
  let rows = items
    .into_iter()
    .map(VmImageRow::from)
    .collect::<Vec<VmImageRow>>();
  match opts.quiet {
    true => {
      for row in rows {
        println!("{}", row.name);
      }
    }
    false => {
      print_table(rows);
    }
  }
  Ok(())
}

/// ## Exec vm image rm
///
/// Function that execute when running `nanocl vm image rm`
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
/// * [names](Vec<String>) The list of vm image names
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_vm_image_rm(
  client: &NanocldClient,
  names: &[String],
) -> IoResult<()> {
  for name in names {
    client.delete_vm_image(name).await?;
  }
  Ok(())
}

/// ## Exec vm image clone
///
/// Function that execute when running `nanocl vm image clone`
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
/// * [name](str) The name of the vm image
/// * [clone_name](str) The name of the clone
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
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

/// ## Exec vm resize
///
/// Function that execute when running `nanocl vm resize`
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
/// * [options](VmImageResizeOpts) The vm image resize options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_vm_resize(
  client: &NanocldClient,
  options: &VmImageResizeOpts,
) -> IoResult<()> {
  let payload = options.clone().into();
  client.resize_vm_image(&options.name, &payload).await?;
  Ok(())
}

/// ## Exec vm image
///
/// Function that execute when running `nanocl vm image`
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
/// * [args](VmImageArg) The vm image arguments
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
pub async fn exec_vm_image(
  client: &NanocldClient,
  args: &VmImageArg,
) -> IoResult<()> {
  match &args.command {
    VmImageCommand::Create(options) => {
      exec_vm_image_create(client, options).await
    }
    VmImageCommand::List(opts) => exec_vm_image_ls(client, opts).await,
    VmImageCommand::Remove { names } => exec_vm_image_rm(client, names).await,
    VmImageCommand::Clone { name, clone_name } => {
      exec_vm_image_clone(client, name, clone_name).await
    }
    VmImageCommand::Resize(opts) => exec_vm_resize(client, opts).await,
  }
}
