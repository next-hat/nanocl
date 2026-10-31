use ntex::http;

use nanocl_error::http::HttpError;
use nanocl_error::http_client::{HttpClientError, HttpClientResult};
use nanocl_error::io::IoError;

fn api_error(
  status: http::StatusCode,
  body: Option<serde_json::Value>,
) -> HttpClientError {
  let message = body.as_ref().and_then(|body| {
    body
      .get("msg")
      .and_then(serde_json::Value::as_str)
      .filter(|msg| !msg.trim().is_empty())
  });
  if let Some(msg) = message {
    return HttpClientError::HttpError(HttpError::new(status, msg));
  }
  let msg = if body.is_some() {
    "Server returned an error without a message"
  } else {
    "Unable to read the server error response"
  };
  if status == http::StatusCode::NOT_FOUND {
    // Do not treat an unrecognized 404 response as a missing object.
    return HttpClientError::IoError(IoError::invalid_data("HTTP 404", msg));
  }
  HttpClientError::HttpError(HttpError::new(status, msg))
}

pub(crate) async fn is_api_error(
  res: &mut ntex::client::ClientResponse,
  status: &http::StatusCode,
) -> HttpClientResult<()> {
  if status.is_server_error() || status.is_client_error() {
    let body = res.json::<serde_json::Value>().await.ok();
    return Err(api_error(*status, body));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn api_error_preserves_status_and_structured_message() {
    let HttpClientError::HttpError(err) = api_error(
      http::StatusCode::FORBIDDEN,
      Some(serde_json::json!({ "msg": "Registry access denied" })),
    ) else {
      panic!("expected HTTP error");
    };
    assert_eq!(err.status, http::StatusCode::FORBIDDEN);
    assert_eq!(err.msg, "Registry access denied");
  }

  #[test]
  fn api_error_preserves_status_when_response_cannot_be_read() {
    let HttpClientError::HttpError(err) =
      api_error(http::StatusCode::BAD_GATEWAY, None)
    else {
      panic!("expected HTTP error");
    };
    assert_eq!(err.status, http::StatusCode::BAD_GATEWAY);
    assert_eq!(err.msg, "Unable to read the server error response");
  }

  #[test]
  fn api_error_has_fallback_for_missing_or_empty_message() {
    for body in [
      serde_json::json!({}),
      serde_json::json!({ "msg": null }),
      serde_json::json!({ "msg": "  " }),
    ] {
      let HttpClientError::HttpError(err) =
        api_error(http::StatusCode::INTERNAL_SERVER_ERROR, Some(body))
      else {
        panic!("expected HTTP error");
      };
      assert_eq!(err.status, http::StatusCode::INTERNAL_SERVER_ERROR);
      assert_eq!(err.msg, "Server returned an error without a message");
    }
  }

  #[test]
  fn api_error_rejects_unrecognized_404_responses() {
    for body in [
      None,
      Some(serde_json::json!({})),
      Some(serde_json::json!({ "msg": null })),
      Some(serde_json::json!({ "msg": "  " })),
    ] {
      let HttpClientError::IoError(err) =
        api_error(http::StatusCode::NOT_FOUND, body)
      else {
        panic!("expected invalid response error");
      };
      assert_eq!(err.inner.kind(), std::io::ErrorKind::InvalidData);
      assert!(err.to_string().contains("HTTP 404"));
      assert!(!err.inner.to_string().is_empty());
    }
  }

  #[test]
  fn api_error_preserves_structured_404_response() {
    let HttpClientError::HttpError(err) = api_error(
      http::StatusCode::NOT_FOUND,
      Some(serde_json::json!({ "msg": "Object not found" })),
    ) else {
      panic!("expected HTTP error");
    };
    assert_eq!(err.status, http::StatusCode::NOT_FOUND);
    assert_eq!(err.msg, "Object not found");
  }
}
