use std::{str::FromStr, io::Error};

use url;
use ntex::http::StatusCode;
use bollard::Docker;
use bollard::image::ImportImageOptions;
use futures::StreamExt;
use tokio::fs::File;
use tokio_util::codec;
use indicatif::{ProgressStyle, ProgressBar};

use crate::error::CliError;
use nanocl_client::error::ApiError;

use super::file;

pub async fn import_tar_from_url(
  docker_api: &Docker,
  url: &str,
) -> Result<(), CliError> {
  let url = url::Url::from_str(url).map_err(|err| ApiError {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    msg: format!("{err}"),
  })?;

  let mut dwres = file::download(&url, "/tmp").await?;
  let style = ProgressStyle::default_spinner();
  let pg = ProgressBar::new(0);
  pg.set_style(style);
  while let Some(chunk) = dwres.stream.next().await {
    if let Err(err) = chunk {
      eprintln!("Error while downloading daemon {err}");
      std::process::exit(1);
    }
    pg.tick();
  }

  let file = File::open(format!("/tmp/{}", &dwres.path)).await?;

  let byte_stream =
    codec::FramedRead::new(file, codec::BytesCodec::new()).map(|r| {
      let bytes = r.unwrap().freeze();
      Ok::<_, Error>(bytes)
    });
  let body = hyper::Body::wrap_stream(byte_stream);
  let mut stream = docker_api.import_image(
    ImportImageOptions {
      ..Default::default()
    },
    body,
    None,
  );

  while let Some(chunk) = stream.next().await {
    if let Err(err) = chunk {
      eprintln!("Error while importing daemon image: {err}");
      std::process::exit(1);
    } else {
      pg.tick();
    }
  }

  pg.finish_and_clear();
  Ok(())
}
