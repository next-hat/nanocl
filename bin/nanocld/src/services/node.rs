use std::rc::Rc;
use std::cell::RefCell;
use std::time::Instant;

use ntex::ws;
use ntex::rt;
use ntex::web;
use ntex::channel::oneshot;
use ntex::{Service, fn_service, pipeline};
use ntex::service::{map_config, fn_shutdown, fn_factory_with_config};
use futures::future::ready;

use crate::models::WsConState;
use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::DaemonState;

#[web::get("/nodes")]
async fn list_node(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let items = repositories::node::list(&state.pool).await?;

  Ok(web::HttpResponse::Ok().json(&items))
}

async fn node_ws_service(
  (sink, state): (ws::WsSink, web::types::State<DaemonState>),
) -> Result<
  impl Service<ws::Frame, Response = Option<ws::Message>, Error = std::io::Error>,
  web::Error,
> {
  // start heartbeat task
  let (tx, rx) = oneshot::channel();
  let con_state = Rc::new(RefCell::new(WsConState::new()));
  rt::spawn(utils::ws::heartbeat(con_state.clone(), sink.clone(), rx));

  // handler service for incoming websockets frames
  let service = fn_service(move |frame| {
    let item = match frame {
      ws::Frame::Ping(msg) => {
        con_state.borrow_mut().hb = Instant::now();
        Some(ws::Message::Pong(msg))
      }
      // update heartbeat
      ws::Frame::Pong(_) => {
        con_state.borrow_mut().hb = Instant::now();
        None
      }
      ws::Frame::Text(text) => {
        println!("Received text: {:#?}", text);
        None
      }
      ws::Frame::Binary(_) => None,
      ws::Frame::Close(reason) => Some(ws::Message::Close(reason)),
      _ => Some(ws::Message::Close(None)),
    };
    ready(Ok(item))
  });

  // handler service for shutdown notification that stop heartbeat task
  let on_shutdown = fn_shutdown(move || {
    let _ = tx.send(());
  });

  // pipe our service with on_shutdown callback
  Ok(pipeline(service).and_then(on_shutdown))
}

/// Entry point for our route
async fn node_ws(
  req: web::HttpRequest,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, web::Error> {
  web::ws::start(
    req,
    // inject chat server send to a ws_service factory
    map_config(fn_factory_with_config(node_ws_service), move |cfg| {
      (cfg, state.clone())
    }),
  )
  .await
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(web::resource("/nodes/ws").route(web::get().to(node_ws)));
}
