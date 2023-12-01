use std::path::Path;
use std::collections::HashMap;

use tokio_util::codec;
use futures::StreamExt;
use bollard_next::service::ProgressDetail;
use indicatif::{ProgressStyle, ProgressBar, MultiProgress};

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocld_client::NanocldClient;

use crate::utils;
use crate::models::{
  CargoImageArg, CargoImageCommand, CargoImageRemoveOpts,
  CargoImageInspectOpts, CargoImageRow, CargoImageImportOpts,
  CargoImageListOpts,
};

/// Function that execute when running `nanocl cargo image ls`
pub(crate) async fn exec_cargo_ls(
  client: &NanocldClient,
  opts: &CargoImageListOpts,
) -> IoResult<()> {
  let items = client.list_cargo_image(Some(&opts.clone().into())).await?;
  let rows = items
    .into_iter()
    .map(CargoImageRow::from)
    .collect::<Vec<CargoImageRow>>();
  match opts.quiet {
    true => {
      for row in rows {
        println!("{}", row.id);
      }
    }
    false => {
      utils::print::print_table(rows);
    }
  }
  Ok(())
}

/// Function that execute when running `nanocl cargo image rm`
async fn exec_cargo_image_rm(
  client: &NanocldClient,
  options: &CargoImageRemoveOpts,
) -> IoResult<()> {
  if !options.skip_confirm {
    utils::dialog::confirm(&format!(
      "Delete cargo images {}?",
      options.names.join(",")
    ))
    .map_err(|err| err.map_err_context(|| "Delete cargo images"))?;
  }
  for name in &options.names {
    client.delete_cargo_image(name).await?;
  }
  Ok(())
}

/// Update the progress bar when pulling an image
fn update_progress(
  multiprogress: &MultiProgress,
  layers: &mut HashMap<String, ProgressBar>,
  id: &str,
  progress: &ProgressDetail,
) {
  let total: u64 = progress
    .total
    .unwrap_or_default()
    .try_into()
    .unwrap_or_default();
  let current: u64 = progress
    .current
    .unwrap_or_default()
    .try_into()
    .unwrap_or_default();
  if let Some(pg) = layers.get(id) {
    let percent = utils::math::calculate_percentage(current, total);
    pg.set_position(percent);
  } else {
    let pg = ProgressBar::new(100);
    let style = ProgressStyle::with_template(
      "[{elapsed_precise}] [{bar:20.cyan/blue}] {pos:>7}% {msg}",
    )
    .unwrap()
    .progress_chars("=> ");
    pg.set_style(style);
    multiprogress.add(pg.to_owned());
    let percent = utils::math::calculate_percentage(current, total);
    pg.set_position(percent);
    layers.insert(id.to_owned(), pg);
  }
}

/// Function that execute when running `nanocl cargo image pull`
pub(crate) async fn exec_cargo_image_pull(
  client: &NanocldClient,
  name: &str,
) -> IoResult<()> {
  let mut stream = client.create_cargo_image(name).await?;
  let mut layers: HashMap<String, ProgressBar> = HashMap::new();
  let multiprogress = MultiProgress::new();
  multiprogress.set_move_cursor(false);
  while let Some(info) = stream.next().await {
    let info = info?;
    // If there is any error we stop the stream
    if let Some(error) = info.error {
      return Err(IoError::interupted("Cargo image create", &error));
    }
    let status = info.status.unwrap_or_default();
    let id = info.id.unwrap_or_default();
    let progress = info.progress_detail.unwrap_or_default();
    match status.as_str() {
      "Pulling fs layer" => {
        update_progress(&multiprogress, &mut layers, &id, &progress);
      }
      "Downloading" => {
        update_progress(&multiprogress, &mut layers, &id, &progress);
      }
      "Download complete" => {
        if let Some(pg) = layers.get(&id) {
          pg.set_position(100);
        }
      }
      "Extracting" => {
        update_progress(&multiprogress, &mut layers, &id, &progress);
      }
      _ => {
        if layers.get(&id).is_none() {
          let _ = multiprogress.println(&status);
        }
      }
    };
    if let Some(pg) = layers.get(&id) {
      pg.set_message(format!("[{}] {}", &id, &status));
    }
  }
  Ok(())
}

/// Function that execute when running `nanocl cargo image inspect`
async fn exec_cargo_image_inspect(
  client: &NanocldClient,
  opts: &CargoImageInspectOpts,
) -> IoResult<()> {
  let image = client.inspect_cargo_image(&opts.name).await?;
  utils::print::print_yml(image)?;
  Ok(())
}

/// Function that execute when running `nanocl cargo image import`
/// To import a cargo/container image from a tarball
async fn exec_cargo_image_import(
  client: &NanocldClient,
  opts: &CargoImageImportOpts,
) -> IoResult<()> {
  let file_path = opts.file_path.clone();
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
  client.import_cargo_image_from_tar(byte_stream).await?;
  Ok(())
}

/// Function that execute when running `nanocl cargo image`
pub(crate) async fn exec_cargo_image(
  client: &NanocldClient,
  opts: &CargoImageArg,
) -> IoResult<()> {
  match &opts.command {
    CargoImageCommand::List(opts) => exec_cargo_ls(client, opts).await,
    CargoImageCommand::Inspect(opts) => {
      exec_cargo_image_inspect(client, opts).await
    }
    CargoImageCommand::Pull(opts) => {
      exec_cargo_image_pull(client, &opts.name).await
    }
    CargoImageCommand::Remove(args) => exec_cargo_image_rm(client, args).await,
    CargoImageCommand::Import(opts) => {
      exec_cargo_image_import(client, opts).await
    }
  }
}
