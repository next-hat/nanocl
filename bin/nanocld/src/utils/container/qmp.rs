use std::future::Future;
use std::io::ErrorKind;
use std::path::Path;

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::vm::VmRuntimeStatus;
use serde_json::{Value, json};
use tokio::io::{
  AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::UnixStream;
use tokio::time::{Instant, timeout_at};

use crate::models::QmpClient;

const MAX_MESSAGE_BYTES: usize = 64 * 1024;

fn invalid(message: &str) -> IoError {
  IoError::invalid_data("QMP", message)
}

async fn before_deadline<T>(
  deadline: Instant,
  operation: impl Future<Output = std::io::Result<T>>,
) -> IoResult<T> {
  // A ready socket must not let an event flood run past the deadline.
  if Instant::now() >= deadline {
    return Err(IoError::with_context(
      "QMP",
      std::io::Error::new(ErrorKind::TimedOut, "Operation deadline exceeded"),
    ));
  }
  timeout_at(deadline, operation)
    .await
    .map_err(|_| {
      IoError::with_context(
        "QMP",
        std::io::Error::new(ErrorKind::TimedOut, "Operation deadline exceeded"),
      )
    })?
    .map_err(|err| IoError::with_context("QMP", err))
}

impl QmpClient {
  pub(crate) async fn connect(
    path: &Path,
    deadline: Instant,
  ) -> IoResult<Self> {
    let stream = before_deadline(deadline, UnixStream::connect(path)).await?;
    Self::handshake(stream, deadline).await
  }
}

impl<S: AsyncRead + AsyncWrite + Unpin> QmpClient<S> {
  async fn handshake(stream: S, deadline: Instant) -> IoResult<Self> {
    let mut client = Self {
      stream: BufReader::new(stream),
      deadline,
      next_id: 1,
      failed: false,
    };
    let greeting = client.read_message().await?;
    let greeting = greeting
      .get("QMP")
      .and_then(Value::as_object)
      .ok_or_else(|| invalid("Missing server greeting"))?;
    if !greeting.get("version").is_some_and(Value::is_object)
      || !greeting
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().all(Value::is_string))
    {
      return Err(invalid("Malformed server greeting"));
    }
    let response = client.execute("qmp_capabilities").await?;
    if !response.is_object() {
      return Err(invalid("Malformed capabilities response"));
    }
    Ok(client)
  }

  /// This reports emulator state, not guest boot or application readiness.
  pub(crate) async fn query_status(&mut self) -> IoResult<VmRuntimeStatus> {
    let response = self.execute("query-status").await?;
    let status = response.get("status").and_then(Value::as_str);
    let running = response.get("running").and_then(Value::as_bool);
    match (running, status) {
      (Some(running), Some(status)) => Ok(VmRuntimeStatus {
        running,
        status: status.to_owned(),
      }),
      _ => {
        self.failed = true;
        Err(invalid("Malformed runtime status response"))
      }
    }
  }

  /// Acknowledgement only: the guest may ignore the shutdown request.
  pub(crate) async fn powerdown(&mut self) -> IoResult<()> {
    let response = self.execute("system_powerdown").await?;
    if !response.is_object() {
      self.failed = true;
      return Err(invalid("Malformed powerdown response"));
    }
    Ok(())
  }

  async fn execute(&mut self, command: &str) -> IoResult<Value> {
    if self.failed {
      return Err(invalid("Session is unusable after an interrupted command"));
    }
    if !matches!(
      command,
      "qmp_capabilities" | "query-status" | "system_powerdown"
    ) {
      return Err(IoError::invalid_input("QMP", "Unsupported command"));
    }
    // Poison before the first await, including if our caller cancels this future.
    self.failed = true;
    let id = self.next_id;
    self.next_id = id
      .checked_add(1)
      .ok_or_else(|| invalid("Request identifier exhausted"))?;
    let mut request = serde_json::to_vec(&json!({
      "execute": command,
      "id": id,
    }))?;
    request.extend_from_slice(b"\r\n");
    before_deadline(self.deadline, self.stream.get_mut().write_all(&request))
      .await?;
    before_deadline(self.deadline, self.stream.get_mut().flush()).await?;
    loop {
      let response = self.read_message().await?;
      if response.get("event").is_some() {
        if !response["event"].is_string()
          || response.get("return").is_some()
          || response.get("error").is_some()
          || response.get("id").is_some()
        {
          return Err(invalid("Malformed event"));
        }
        // Events are consumed without buffering; the caller observes final state.
        continue;
      }
      let response_id = response
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("Missing response identifier"))?;
      if response_id != id {
        // Never mistake a stale/unknown response for this command's result.
        continue;
      }
      match (response.get("return"), response.get("error")) {
        (Some(result), None) => {
          self.failed = false;
          return Ok(result.clone());
        }
        (None, Some(error)) => {
          let class = error
            .get("class")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Malformed command error"))?;
          if !error.get("desc").is_some_and(Value::is_string) {
            return Err(invalid("Malformed command error"));
          }
          // Do not echo arbitrary server descriptions or unbounded input.
          let class: String = class
            .chars()
            .take(64)
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
          return Err(IoError::other(
            "QMP",
            &format!("Command {command} failed ({class})"),
          ));
        }
        _ => return Err(invalid("Malformed command response")),
      }
    }
  }

  async fn read_message(&mut self) -> IoResult<Value> {
    let mut line = Vec::new();
    loop {
      let bytes =
        before_deadline(self.deadline, self.stream.fill_buf()).await?;
      if bytes.is_empty() {
        return Err(IoError::with_context(
          "QMP",
          std::io::Error::new(ErrorKind::UnexpectedEof, "Server disconnected"),
        ));
      }
      let end = bytes.iter().position(|byte| *byte == b'\n');
      let count = end.map_or(bytes.len(), |end| end + 1);
      if line.len() + count > MAX_MESSAGE_BYTES {
        return Err(invalid("Message exceeds 64 KiB"));
      }
      line.extend_from_slice(&bytes[..count]);
      self.stream.consume(count);
      if end.is_some() {
        let value: Value = serde_json::from_slice(&line)
          .map_err(|_| invalid("Malformed JSON message"))?;
        if !value.is_object() {
          return Err(invalid("Expected a JSON object"));
        }
        return Ok(value);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use tokio::io::{DuplexStream, duplex};

  use super::*;

  fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
  }

  async fn send(peer: &mut BufReader<DuplexStream>, message: Value) {
    let mut bytes = serde_json::to_vec(&message).unwrap();
    bytes.push(b'\n');
    peer.get_mut().write_all(&bytes).await.unwrap();
  }

  async fn receive(peer: &mut BufReader<DuplexStream>) -> Value {
    let mut line = String::new();
    peer.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
  }

  async fn greet(peer: &mut BufReader<DuplexStream>) {
    send(peer, json!({"QMP": {"version": {}, "capabilities": []}})).await;
    let request = receive(peer).await;
    assert_eq!(request["execute"], "qmp_capabilities");
    send(peer, json!({"return": {}, "id": request["id"]})).await;
  }

  #[ntex::test]
  async fn qmp_negotiates_correlates_and_consumes_events() {
    let (stream, peer) = duplex(1024);
    let client = async {
      let mut client = QmpClient::handshake(stream, deadline()).await.unwrap();
      let status = client.query_status().await.unwrap();
      assert!(status.running);
      assert_eq!(status.status, "running");
      client.powerdown().await.unwrap();
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      let request = receive(&mut peer).await;
      assert_eq!(request["execute"], "query-status");
      send(&mut peer, json!({"event": "RESUME"})).await;
      send(
        &mut peer,
        json!({"return": {"running": false, "status": "paused"}, "id": 999}),
      )
      .await;
      send(&mut peer, json!({"return": {"running": true, "status": "running"}, "id": request["id"]})).await;
      let powerdown = receive(&mut peer).await;
      assert_eq!(powerdown["execute"], "system_powerdown");
      assert_ne!(request["id"], powerdown["id"]);
      send(&mut peer, json!({"return": {}, "id": powerdown["id"]})).await;
      // No SHUTDOWN event: the method must return acknowledgement only.
    };
    futures::join!(client, server);
  }

  #[ntex::test]
  async fn qmp_reports_command_error_without_server_payload() {
    let (stream, peer) = duplex(1024);
    let client = async {
      let mut client = QmpClient::handshake(stream, deadline()).await.unwrap();
      let err = client.powerdown().await.unwrap_err();
      assert!(err.to_string().contains("GenericError"));
      assert!(!err.to_string().contains("private server payload"));
      assert!(client.query_status().await.is_err());
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      let request = receive(&mut peer).await;
      send(&mut peer, json!({"error": {"class": "GenericError", "desc": "private server payload"}, "id": request["id"]})).await;
    };
    futures::join!(client, server);
  }

  #[ntex::test]
  async fn qmp_rejects_malformed_greeting_and_status() {
    for message in [b"not json\n".as_slice(), b"[]\n", b"{\"QMP\":{}}\n"] {
      let (stream, mut peer) = duplex(1024);
      peer.write_all(message).await.unwrap();
      let err = QmpClient::handshake(stream, deadline())
        .await
        .err()
        .unwrap();
      assert_eq!(err.inner.kind(), ErrorKind::InvalidData);
    }
    let (stream, peer) = duplex(1024);
    let client = async {
      let mut client = QmpClient::handshake(stream, deadline()).await.unwrap();
      let err = client.query_status().await.unwrap_err();
      assert_eq!(err.inner.kind(), ErrorKind::InvalidData);
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      let request = receive(&mut peer).await;
      send(&mut peer, json!({"return": {"running": "true", "status": "running"}, "id": request["id"]})).await;
    };
    futures::join!(client, server);
  }

  #[ntex::test]
  async fn qmp_bounds_message_size_and_rejects_eof() {
    let (stream, mut peer) = duplex(MAX_MESSAGE_BYTES + 1);
    peer
      .write_all(&vec![b' '; MAX_MESSAGE_BYTES + 1])
      .await
      .unwrap();
    let err = QmpClient::handshake(stream, deadline())
      .await
      .err()
      .unwrap();
    assert_eq!(err.inner.kind(), ErrorKind::InvalidData);
    assert!(err.to_string().contains("64 KiB"));

    let (stream, mut peer) = duplex(1024);
    peer.write_all(b"{\"QMP\":").await.unwrap();
    drop(peer);
    let err = QmpClient::handshake(stream, deadline())
      .await
      .err()
      .unwrap();
    assert_eq!(err.inner.kind(), ErrorKind::UnexpectedEof);
  }

  #[ntex::test]
  async fn qmp_keeps_one_deadline_after_negotiation() {
    let (stream, peer) = duplex(1024);
    let expires = deadline();
    let client = async {
      let mut client = QmpClient::handshake(stream, expires).await.unwrap();
      assert_eq!(client.deadline, expires);
      // An expired operation may not perform even a ready write/read.
      client.deadline = Instant::now();
      let err = client.query_status().await.unwrap_err();
      assert_eq!(err.inner.kind(), ErrorKind::TimedOut);
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      let mut line = String::new();
      assert_eq!(peer.read_line(&mut line).await.unwrap(), 0);
    };
    futures::join!(client, server);

    let (stream, _peer) = duplex(1024);
    let err = QmpClient::handshake(stream, Instant::now())
      .await
      .err()
      .unwrap();
    assert_eq!(err.inner.kind(), ErrorKind::TimedOut);
  }
}
