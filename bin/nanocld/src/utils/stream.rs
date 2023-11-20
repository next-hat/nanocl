use ntex::http;
use ntex::util::Bytes;
use futures::StreamExt;
use serde::Serialize;

use nanocl_error::http::{HttpResult, HttpError};

/// ## Transform stream
///
/// Transform a stream of items serializable in json into a stream of bytes
///
/// ## Arguments
///
/// * [stream](impl StreamExt<Item = Result<I, impl std::error::Error>>) The stream to transform
///   * [I](I) - The type of the stream items
///   * [T](T) - The type to transform the stream items into
///
/// ## Return
///
/// [Stream](StreamExt) of [HttpResult](HttpResult) of [Bytes](Bytes)
///
pub(crate) fn transform_stream<I, T>(
  stream: impl StreamExt<Item = Result<I, impl std::error::Error>>,
) -> impl StreamExt<Item = HttpResult<Bytes>>
where
  I: Into<T>,
  T: Serialize + From<I>,
{
  stream.map(|item| {
    let item = item.map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to read stream item: {err}"),
    })?;
    let item = T::from(item);
    let item = serde_json::to_string(&item).map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to serialize stream item: {err}"),
    })?;
    Ok(Bytes::from(item + "\r\n"))
  })
}
