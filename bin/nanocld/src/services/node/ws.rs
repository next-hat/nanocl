use std::{cell::RefCell, rc::Rc, time::Instant};

use futures::future::ready;
use ntex::{
  chain,
  channel::oneshot,
  fn_service, rt,
  service::{fn_factory_with_config, fn_shutdown, map_config},
  util::ByteString,
  web, ws, Service,
};

use crate::{
  models::{SystemState, WsConState},
  utils,
};

async fn node_ws_service(
  (sink, state): (ws::WsSink, web::types::State<SystemState>),
) -> Result<
  impl Service<ws::Frame, Response = Option<ws::Message>, Error = std::io::Error>,
  web::Error,
> {
  // start heartbeat task
  let (tx, rx) = oneshot::channel();
  let con_state = Rc::new(RefCell::new(WsConState::new()));
  rt::spawn(utils::ws::heartbeat(con_state.clone(), sink.clone(), rx));
  let message = format!("[SERVER] hello i'm {}", state.inner.config.hostname);
  let _ = sink
    .send(ws::Message::Text(ByteString::from(message)))
    .await;
  // handler service for incoming web sockets frames
  let service = fn_service(move |frame| {
    let item = match frame {
      ws::Frame::Ping(msg) => {
        con_state.borrow_mut().hb = Instant::now();
        Some(ws::Message::Pong(msg))
      }
      ws::Frame::Pong(_) => {
        // update heartbeat time
        con_state.borrow_mut().hb = Instant::now();
        None
      }
      ws::Frame::Close(reason) => Some(ws::Message::Close(reason)),
      _ => None,
    };
    ready(Ok(item))
  });
  // handler service for shutdown notification that stop heartbeat task
  let on_shutdown = fn_shutdown(move || {
    let _ = tx.send(());
  });
  // pipe our service with on_shutdown callback
  Ok(chain(service).and_then(on_shutdown))
}

/// Websocket endpoint for communication between nodes used internally
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Nodes",
  path = "/nodes/ws",
  responses(
    (status = 101, description = "Websocket connection"),
  ),
))]
pub async fn node_ws(
  state: web::types::State<SystemState>,
  req: web::HttpRequest,
) -> Result<web::HttpResponse, web::Error> {
  web::ws::start(
    req,
    // inject state to the node_ws_service
    map_config(fn_factory_with_config(node_ws_service), move |cfg| {
      (cfg, state.clone())
    }),
  )
  .await
}
