use std::rc::Rc;
use std::cell::RefCell;
use std::time::Instant;

use ntex::{rt, ws, web};
use ntex::channel::oneshot;
use ntex::util::ByteString;
use ntex::{Service, fn_service, chain};
use ntex::service::{map_config, fn_shutdown, fn_factory_with_config};
use futures::future::ready;

use nanocl_error::http::HttpError;

use crate::{utils, repositories};
use crate::models::{DaemonState, WsConState};

/// List nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Nodes",
  path = "/nodes",
  responses(
    (status = 200, description = "List of nodes", body = [Node]),
  ),
))]
#[web::get("/nodes")]
pub(crate) async fn list_node(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
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
  let message = format!("[SERVER] hello i'm {}", state.config.hostname);
  let _ = sink
    .send(ws::Message::Text(ByteString::from(message)))
    .await;
  // handler service for incoming websockets frames
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
async fn node_ws(
  req: web::HttpRequest,
  state: web::types::State<DaemonState>,
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

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_node);
  config.service(web::resource("/nodes/ws").route(web::get().to(node_ws)));
}

#[cfg(test)]
mod tests {

  use ntex::http;

  use nanocl_stubs::node::Node;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/nodes";

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list nodes");
    let _ = res.json::<Vec<Node>>().await.unwrap();
  }
}
