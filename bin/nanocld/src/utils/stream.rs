use ntex::util::Bytes;
use futures::StreamExt;
use serde::Serialize;

use nanocl_error::http::{HttpError, HttpResult};

/// Transform a stream of items serializable in json into a stream of bytes
pub fn transform_stream<I, T>(
  stream: impl StreamExt<Item = Result<I, impl std::error::Error>>,
) -> impl StreamExt<Item = HttpResult<Bytes>>
where
  I: Into<T>,
  T: Serialize + From<I>,
{
  stream.map(|item| {
    let item = item.map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed to read stream item: {err}"
      ))
    })?;
    let item = T::from(item);
    let item = serde_json::to_string(&item).map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed stringify stream item: {err}"
      ))
    })?;
    Ok(Bytes::from(item + "\r\n"))
  })
}
