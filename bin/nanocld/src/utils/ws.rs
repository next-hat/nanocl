use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use ntex::channel::oneshot;
use ntex::time;
use ntex::util;
use ntex::{web, ws};

use nanocl_error::http::{HttpError, HttpResult};

use crate::models::{CLIENT_TIMEOUT, HEARTBEAT_INTERVAL, WsConState};

/// ## Heartbeat
///
/// Websocket helper method that sends ping to client every second.
/// Also this method checks heartbeats from client.
///
/// ## Arguments
///
/// * [state](Rc<RefCell<WsConState>>) Reference to websocket connection state
/// * [sink](ws::WsSink) Reference to websocket sink
/// * [rx](oneshot::Receiver<()>) Reference to oneshot receiver
///
pub async fn heartbeat(
  state: Rc<RefCell<WsConState>>,
  sink: ws::WsSink,
  mut rx: oneshot::Receiver<()>,
) {
  loop {
    match util::select(Box::pin(time::sleep(HEARTBEAT_INTERVAL)), &mut rx).await
    {
      util::Either::Left(_) => {
        // check client heartbeats
        if Instant::now().duration_since(state.borrow().hb) > CLIENT_TIMEOUT {
          // heartbeat timed out
          log::debug!("Websocket Client heartbeat failed, disconnecting!");
          // disconnect connection
          sink.io().close();
          return;
        } else {
          // send ping
          if sink
            .send(ws::Message::Ping(util::Bytes::new()))
            .await
            .is_err()
          {
            return;
          }
        }
      }
      util::Either::Right(_) => {
        log::debug!("Connection is dropped, stop heartbeat task");
        return;
      }
    }
  }
}

/// Preserve handshake status codes for HTTP requests that were not upgraded.
/// Once upgraded, ntex has already sent the handshake and ignores this response.
pub fn response(
  result: Result<(), ws::error::WsError<std::io::Error>>,
) -> HttpResult<web::HttpResponse> {
  match result {
    Ok(()) => Ok(web::HttpResponse::Ok().build()),
    Err(ws::error::WsError::Handshake(error)) => {
      Ok(ntex::http::error::ResponseError::error_response(&error))
    }
    Err(error) => Err(HttpError::internal_server_error(error)),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use ntex::http::StatusCode;
  use ntex::web::test::{TestRequest, init_service};

  #[ntex::test]
  async fn websocket_request_without_upgrade_is_bad_request() {
    let app = init_service(web::App::default().route(
      "/attach",
      web::get().to(async |req: web::HttpRequest| {
        response(
          web::ws::start(&req, None, async |_frame| {
            Ok::<_, std::io::Error>(None)
          })
          .await,
        )
      }),
    ))
    .await;
    let result = app
      .call(TestRequest::with_uri("/attach").to_request())
      .await
      .unwrap();
    assert_eq!(result.status(), StatusCode::BAD_REQUEST);
  }
}
