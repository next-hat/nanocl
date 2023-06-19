use std::rc::Rc;
use std::cell::RefCell;
use std::time::Instant;

use ntex::ws;
use ntex::time;
use ntex::util;
use ntex::channel::oneshot;

use crate::models::{WsConState, HEARTBEAT_INTERVAL, CLIENT_TIMEOUT};

/// ## Heartbeat
///
/// Websocket helper method that sends ping to client every second.
/// Also this method checks heartbeats from client.
///
/// ## Arguments
///
/// - [state](Rc<RefCell<WsConState>>) Reference to websocket connection state
/// - [sink](ws::WsSink) Reference to websocket sink
/// - [rx](oneshot::Receiver<()>) Reference to oneshot receiver
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
