use ntex::channel::mpsc::Receiver;
use ntex::channel::mpsc::Sender;
use ntex::io::Base;
use ntex::rt;
use ntex::time;
use ntex::ws;
use ntex::util::Bytes;
use ntex::http::StatusCode;
use futures::StreamExt;
use ntex::ws::WsConnection;

use crate::version::VERSION;
use crate::error::HttpResponseError;
use crate::models::HEARTBEAT_INTERVAL;

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

  pub async fn connect_to(
    &self,
  ) -> Result<WsConnection<Base>, HttpResponseError> {
    let url = format!("http://{}/{VERSION}/nodes/ws", self.ip_addr);
    let con = ws::WsClient::build(url)
      .finish()
      .map_err(|err| HttpResponseError {
        msg: format!("Failed to build websocket connection: {}", err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?
      .connect()
      .await
      .map_err(|err| HttpResponseError {
        msg: format!("Failed to connect to websocket: {}", err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;

    Ok(con)
  }
}

pub struct Node(Receiver<Result<Bytes, HttpResponseError>>);

pub struct NodeServer {}
