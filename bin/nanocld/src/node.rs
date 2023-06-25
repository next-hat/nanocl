use std::time::Duration;
use std::collections::HashMap;

use ntex::rt;
use ntex::ws;
use ntex::http;
use ntex::time;
use ntex::io::Base;
use ntex::util::Bytes;
use ntex::ws::WsConnection;

use futures::SinkExt;
use futures::StreamExt;
use futures::channel::mpsc;

use nanocl_utils::io_error::IoResult;
use nanocl_utils::http_error::HttpError;
use nanocl_stubs::config::DaemonConfig;

use crate::repositories;
use crate::version::VERSION;
use crate::models::{DaemonState, NodeDbModel};

#[derive(Clone)]
pub struct NodeMessage {
  data: String,
}

#[derive(Debug, Clone)]
pub struct NodeClient {
  ip_addr: String,
}

impl NodeClient {
  pub fn new(ip_addr: &str) -> Self {
    Self {
      ip_addr: ip_addr.to_owned(),
    }
  }

  pub async fn connect(&self) -> Result<WsConnection<Base>, HttpError> {
    let url = format!("http://{}/{VERSION}/nodes/ws", self.ip_addr);
    let con = ws::WsClient::build(url)
      .finish()
      .map_err(|err| HttpError {
        msg: format!("Failed to build websocket connection: {}", err),
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
      })?
      .connect()
      .await
      .map_err(|err| HttpError {
        msg: format!("Failed to connect to websocket: {}", err),
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
      })?;
    Ok(con)
  }
}

pub enum NodeClientsMessage {
  Connect {
    node_id: String,
    sender: mpsc::UnboundedSender<NodeMessage>,
  },
  SendMessage {
    node_id: String,
    msg: NodeMessage,
  },
  ReceiveMessage {
    msg: NodeMessage,
  },
}

#[derive(Default)]
pub struct NodeClients {
  sessions: HashMap<String, mpsc::UnboundedSender<NodeMessage>>,
}

impl NodeClients {
  fn handle(&mut self, msg: NodeClientsMessage) {
    match msg {
      NodeClientsMessage::Connect { node_id, sender } => {
        self.sessions.insert(node_id, sender);
      }
      NodeClientsMessage::SendMessage { node_id: _, msg } => {
        for sender in self.sessions.values() {
          let mut sender = sender.clone();
          let msg = msg.clone();
          rt::spawn(async move {
            let _ = sender.send(msg).await;
          });
        }
      }
      #[allow(unused_variables)]
      NodeClientsMessage::ReceiveMessage { msg } => {
        log::debug!("Received message from node: {}", msg.data);
      }
    }
  }
}

/// Handle messages from chat server, we simply send it to the peer websocket connection
async fn messages(
  sink: ws::WsSink,
  mut server: mpsc::UnboundedReceiver<NodeMessage>,
) {
  while let Some(msg) = server.next().await {
    let _ = sink.send(ws::Message::Text(msg.data.into())).await;
  }
}

pub fn watch_node(
  daemon_conf: &DaemonConfig,
  node: &NodeDbModel,
  mut srv: mpsc::UnboundedSender<NodeClientsMessage>,
) {
  let node = node.clone();
  let daemon_conf = daemon_conf.clone();
  rt::spawn(async move {
    loop {
      let client = NodeClient::new(&node.ip_address);
      match client.connect().await {
        Ok(con) => {
          // start heartbeat task
          log::info!(
            "Successfully connected to node {} at {}",
            &node.name,
            &node.ip_address
          );
          let (tx, rx) = mpsc::unbounded::<NodeMessage>();

          let _ = srv
            .send(NodeClientsMessage::Connect {
              node_id: node.name.clone(),
              sender: tx,
            })
            .await;
          // start server messages handler, it reads chat messages and sends to the peer
          rt::spawn(messages(con.sink(), rx));

          let _ = srv
            .send(NodeClientsMessage::SendMessage {
              node_id: node.name.clone(),
              msg: NodeMessage {
                data: format!("[CLIENT] Hello i'm {}", daemon_conf.hostname),
              },
            })
            .await;

          let sink = con.sink();
          let mut stream = con.seal().receiver();
          while let Some(frame) = stream.next().await {
            let frame = match frame {
              Ok(frame) => frame,
              Err(err) => {
                log::warn!(
                  "Failed to read frame from node {} {}: {}",
                  &node.name,
                  &node.ip_address,
                  err
                );
                break;
              }
            };
            match frame {
              ws::Frame::Binary(msg) => {
                let msg = String::from_utf8(msg.to_vec()).unwrap();
                let _ = srv
                  .send(NodeClientsMessage::ReceiveMessage {
                    msg: NodeMessage { data: msg },
                  })
                  .await;
              }
              ws::Frame::Text(msg) => {
                let msg = String::from_utf8(msg.to_vec()).unwrap();
                let _ = srv
                  .send(NodeClientsMessage::ReceiveMessage {
                    msg: NodeMessage { data: msg },
                  })
                  .await;
              }
              ws::Frame::Ping(_) => {
                let _ = sink.send(ws::Message::Pong(Bytes::new())).await;
              }
              _ => {
                log::warn!(
                  "Received invalid frame from node {} {}: {:?}",
                  &node.name,
                  &node.ip_address,
                  frame
                );
              }
            }
          }
        }
        Err(err) => {
          log::warn!(
            "Failed to connect to node {} {}: {}",
            &node.name,
            &node.ip_address,
            err
          );
        }
      }
      log::warn!(
        "Retrying to connect to Node {} at {} in 5 seconds",
        &node.name,
        &node.ip_address
      );
      time::sleep(Duration::from_secs(5)).await;
    }
  });
}

pub async fn register(daemon_state: &DaemonState) -> IoResult<()> {
  let node = NodeDbModel {
    name: daemon_state.config.hostname.clone(),
    ip_address: daemon_state.config.gateway.clone(),
  };
  repositories::node::create_if_not_exists(&node, &daemon_state.pool).await?;
  Ok(())
}

pub async fn join_cluster(state: &DaemonState) -> IoResult<()> {
  let state = state.clone();
  let (tx, mut rx) = mpsc::unbounded();
  let mut clients = NodeClients::default();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      while let Some(msg) = rx.next().await {
        clients.handle(msg);
      }
      rt::Arbiter::current().stop();
    });
  });
  let nodes =
    repositories::node::list_unless(&state.config.hostname, &state.pool)
      .await?;
  for node in nodes {
    log::info!("Connecting to node {} at {}", node.name, node.ip_address);
    watch_node(&state.config, &node, tx.clone());
  }
  Ok(())
}
