use std::fs;
use std::os::unix::prelude::FileExt;
use std::path::Path;
use std::str::FromStr;
use futures::SinkExt;
use futures::TryStreamExt;
use ntex::http::StatusCode;
use url::Url;
use ntex::web;
use ntex::rt;
use ntex::http::Client;
use serde::{Serialize, Deserialize};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};

use crate::client::error::ApiError;

#[derive(Debug, Serialize, Deserialize)]
pub enum DownloadFileStatus {
  Downloading,
  Syncing,
  Done,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadFileInfo {
  pub(crate) percent: f64,
  pub(crate) status: DownloadFileStatus,
}

impl Default for DownloadFileInfo {
  fn default() -> Self {
    Self {
      percent: 0.0,
      status: DownloadFileStatus::Downloading,
    }
  }
}

impl DownloadFileInfo {
  fn new(percent: f64, status: DownloadFileStatus) -> Self {
    Self { percent, status }
  }
}

pub struct DownloadFileRes {
  pub(crate) path: String,
  pub(crate) stream: UnboundedReceiver<Result<DownloadFileInfo, ApiError>>,
}

/// # Download file
/// Download a file over http protocol for given url in given directory
pub async fn download(
  url: &Url,
  download_dir: impl AsRef<Path>,
) -> Result<DownloadFileRes, ApiError> {
  // ubuntu cloud server doesn't return any filename in headers so i use the path to dertermine the file name
  // a test should be made to see if the header containt filename to use it instead of the path
  let file_name = url
    .path_segments()
    .ok_or_else(|| ApiError {
      status: StatusCode::BAD_REQUEST,
      msg: String::from("url have empty path cannot determine file name lol."),
    })?
    .last()
    .ok_or_else(|| ApiError {
      status: StatusCode::BAD_REQUEST,
      msg: String::from("url have empty path cannot determine file name lol."),
    })?;
  let client = Client::build()
    .timeout(ntex::time::Millis::from_secs(2000))
    .finish();
  let mut res =
    client
      .get(url.to_string())
      .send()
      .await
      .map_err(|err| ApiError {
        status: StatusCode::BAD_REQUEST,
        msg: format!("Unable to get {:?} {:?}", &url, &err),
      })?;
  let mut status = res.status();

  if status.is_redirection() {
    let url = res.header("Location").unwrap().to_str().unwrap();
    let url = url::Url::from_str(url).unwrap();
    res = client
      .get(url.to_string())
      .send()
      .await
      .map_err(|err| ApiError {
        status: StatusCode::BAD_REQUEST,
        msg: format!("Unable to get {:?} {:?}", &url, &err),
      })?;
    status = res.status();
  }

  if !status.is_success() {
    return Err(ApiError {
      status,
      msg: format!("Unable to get {:?} got response error {:?}", &url, &status),
    });
  }
  let total_size = res
    .header("content-length")
    .ok_or_else(|| ApiError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("Unable to download {:?} content-length not set.", &url),
    })?
    .to_str()
    .map_err(|err| ApiError {
      status: StatusCode::BAD_REQUEST,
      msg: format!(
        "Unable to download {:?} cannot convert content-length got error {:?}",
        &url, &err
      ),
    })?
    .parse::<u64>()
    .map_err(|err| ApiError {
      status: StatusCode::BAD_REQUEST,
      msg: format!(
        "Unable to download {:?} cannot convert content-length got error {:?}",
        &url, &err
      ),
    })?;
  let url = url.to_owned();
  let file_path = Path::new(download_dir.as_ref()).join(file_name);
  let ret_file_path = file_name.to_owned();
  let (mut wtx, wrx) = unbounded::<Result<DownloadFileInfo, ApiError>>();
  rt::spawn(async move {
    let mut stream = res.into_stream();
    let fp = file_path.to_owned();
    let file = web::block(move || {
      let file = fs::File::create(&fp).map_err(|err| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Unable to create file {:?} got error {:?}", &fp, &err),
      })?;
      Ok::<_, ApiError>(file)
    })
    .await
    .map_err(|err| ApiError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("{}", err),
    })?;
    let mut offset: u64 = 0;
    while let Some(chunk) = stream.try_next().await.map_err(|err| ApiError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!(
        "Unable to load stream from {:?} got error {:?}",
        &url, &err
      ),
    })? {
      file.write_at(&chunk, offset).map_err(|err| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!(
          "Unable to write in file {:?} got error {:?}",
          &file_path, &err
        ),
      })?;
      offset += chunk.len() as u64;
      let percent = (offset as f64 / total_size as f64) * 100.0;
      let info =
        DownloadFileInfo::new(percent, DownloadFileStatus::Downloading);
      let send = wtx.send(Ok::<_, ApiError>(info)).await;
      if let Err(_err) = send {
        break;
      }
    }
    if offset == total_size {
      file.sync_all().map_err(|err| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!(
          "Unable to sync file {:?} got error {:?}",
          &file_path, &err
        ),
      })?;
      let info = DownloadFileInfo::new(100.0, DownloadFileStatus::Done);
      let _send = wtx.send(Ok::<_, ApiError>(info)).await;
    } else {
      fs::remove_file(&file_path).map_err(|err| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Unable to delete created file {:?}", err),
      })?;
    }
    Ok::<(), ApiError>(())
  });
  let res = DownloadFileRes {
    path: ret_file_path,
    stream: wrx,
  };
  Ok(res)
}
