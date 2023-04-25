use ntex::util::Bytes;
use ntex::http::StatusCode;
use futures::StreamExt;
use serde::Serialize;

use nanocl_utils::http_error::HttpError;

pub(crate) fn transform_stream<I, T>(
  stream: impl StreamExt<Item = Result<I, impl std::error::Error>>,
) -> impl StreamExt<Item = Result<Bytes, HttpError>>
where
  I: Into<T>,
  T: Serialize + From<I>,
{
  stream.map(|item| {
    let item = item.map_err(|err| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to read stream item: {err}"),
    })?;
    let item = T::from(item);
    let item = serde_json::to_string(&item).map_err(|err| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to serialize stream item: {err}"),
    })?;
    Ok(Bytes::from(item + "\r\n"))
  })
}
