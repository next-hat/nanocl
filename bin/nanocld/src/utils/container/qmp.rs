use std::future::Future;
use std::io::ErrorKind;
use std::path::Path;

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::vm::VmRuntimeStatus;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{
  AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::UnixStream;
use tokio::time::{Instant, timeout_at};

use crate::models::{
  QmpBlockDevice, QmpBlockNode, QmpClient, QmpCpuSlot, QmpMemoryDevice,
  QmpMemorySize,
};

const MAX_MESSAGE_BYTES: usize = 64 * 1024;

fn invalid(message: &str) -> IoError {
  IoError::invalid_data("QMP", message)
}

fn managed_id(id: &str, prefix: &str) -> IoResult<()> {
  if id.len() > 96
    || id.len() <= prefix.len()
    || !id.starts_with(prefix)
    || !id
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
  {
    return Err(IoError::invalid_input(
      "QMP",
      "Invalid managed device identifier",
    ));
  }
  Ok(())
}

fn resource_size(size: u64) -> IoResult<()> {
  if size == 0 || size > i64::MAX as u64 {
    return Err(IoError::invalid_input("QMP", "Invalid resource size"));
  }
  Ok(())
}

/// Shared by resize preflight and the final QMP mutation check.
pub(crate) fn validate_disk_growth(
  block: &QmpBlockNode,
  size: u64,
) -> IoResult<()> {
  resource_size(size)?;
  let image = &block.image;
  let format_safe = match (image.format.as_str(), &image.format_specific) {
    ("raw", None) => true,
    ("qcow2", Some(format)) => {
      format.get("type").and_then(Value::as_str) == Some("qcow2")
        && format
          .get("data")
          .and_then(Value::as_object)
          .is_some_and(|data| {
            !data.contains_key("encrypt")
              && !data.contains_key("data-file")
              && data
                .get("corrupt")
                .is_none_or(|value| value.as_bool() == Some(false))
          })
    }
    _ => false,
  };
  if block.ro
    || block.encrypted
    || block.backing_file_depth != 0
    || block.backing_file.is_some()
    || block.drv != image.format
    || image.encrypted == Some(true)
    || image.backing_filename.is_some()
    || image.full_backing_filename.is_some()
    || image.backing_image.is_some()
    || image
      .snapshots
      .as_ref()
      .is_some_and(|snapshots| !snapshots.is_empty())
    || !format_safe
  {
    return Err(IoError::invalid_input(
      "QMP",
      "Unsupported block image for live growth",
    ));
  }
  if size < image.virtual_size {
    return Err(IoError::invalid_input(
      "QMP",
      "Disk shrinking is not supported",
    ));
  }
  Ok(())
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

  pub(crate) async fn query_cpu_slots(&mut self) -> IoResult<Vec<QmpCpuSlot>> {
    self.query("query-hotpluggable-cpus").await
  }

  pub(crate) async fn query_cpus_count(&mut self) -> IoResult<u64> {
    let cpus = self.execute("query-cpus-fast").await?;
    let Some(cpus) = cpus.as_array() else {
      self.failed = true;
      return Err(invalid("Malformed CPU inventory response"));
    };
    let mut indices = std::collections::BTreeSet::new();
    for cpu in cpus {
      let index = cpu.get("cpu-index").and_then(Value::as_u64);
      if !index.is_some_and(|index| indices.insert(index)) {
        self.failed = true;
        return Err(invalid("Malformed CPU inventory response"));
      }
    }
    Ok(cpus.len() as u64)
  }

  pub(crate) async fn query_memory_size(&mut self) -> IoResult<QmpMemorySize> {
    self.query("query-memory-size-summary").await
  }

  pub(crate) async fn query_memory_devices(
    &mut self,
  ) -> IoResult<Vec<QmpMemoryDevice>> {
    self.query("query-memory-devices").await
  }

  pub(crate) async fn query_block_devices(
    &mut self,
  ) -> IoResult<Vec<QmpBlockDevice>> {
    self.query("query-block").await
  }

  /// The caller selects an unoccupied slot from the current runtime inventory.
  pub(crate) async fn add_cpu(
    &mut self,
    id: &str,
    slot: &QmpCpuSlot,
  ) -> IoResult<()> {
    managed_id(id, "nanocl-cpu-")?;
    if slot.qom_path.is_some()
      || slot.vcpus_count != 1
      || slot.driver.len() > 96
      || !slot.driver.ends_with("-x86_64-cpu")
      || !slot.driver.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
      })
      || slot.props.is_empty()
      || slot.props.iter().any(|(key, value)| {
        !matches!(
          key.as_str(),
          "node-id"
            | "socket-id"
            | "die-id"
            | "cluster-id"
            | "core-id"
            | "thread-id"
        ) || *value > i64::MAX as u64
      })
    {
      return Err(IoError::invalid_input("QMP", "Unsupported CPU slot"));
    }
    let mut arguments = json!({"driver": slot.driver, "id": id});
    for (key, value) in &slot.props {
      arguments[key] = json!(value);
    }
    self.mutate("device_add", arguments).await
  }

  pub(crate) async fn add_memory_backend(
    &mut self,
    id: &str,
    size: u64,
  ) -> IoResult<()> {
    managed_id(id, "nanocl-mem-")?;
    resource_size(size)?;
    self
      .mutate(
        "object-add",
        json!({"qom-type": "memory-backend-ram", "id": id, "size": size}),
      )
      .await
  }

  pub(crate) async fn add_memory_device(
    &mut self,
    id: &str,
    backend: &str,
  ) -> IoResult<()> {
    managed_id(id, "nanocl-dimm-")?;
    managed_id(backend, "nanocl-mem-")?;
    self
      .mutate(
        "device_add",
        json!({"driver": "pc-dimm", "id": id, "memdev": backend}),
      )
      .await
  }

  /// Use a fresh session after a failed add, and verify this owned backend is
  /// unattached first. QEMU also rejects deleting an in-use memory backend.
  pub(crate) async fn delete_memory_backend(
    &mut self,
    id: &str,
  ) -> IoResult<()> {
    managed_id(id, "nanocl-mem-")?;
    self.mutate("object-del", json!({"id": id})).await
  }

  /// Verify the active block node again before requesting growth. Filesystem
  /// ownership and guest partition/filesystem growth belong to the caller.
  pub(crate) async fn resize_block(
    &mut self,
    node: &str,
    size: u64,
  ) -> IoResult<()> {
    resource_size(size)?;
    if node.is_empty()
      || node.len() > 128
      || !node.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
          || matches!(byte, b'_' | b'-' | b'.' | b'#')
      })
    {
      return Err(IoError::invalid_input(
        "QMP",
        "Invalid block node identifier",
      ));
    }
    let devices = self.query_block_devices().await?;
    let mut matches = devices.iter().filter_map(|device| {
      device
        .inserted
        .as_ref()
        .filter(|inserted| inserted.node_name == node)
        .map(|inserted| (device, inserted))
    });
    let Some((device, block)) = matches.next() else {
      return Err(invalid("Block node is not attached"));
    };
    if matches.next().is_some() {
      return Err(invalid("Block node is attached more than once"));
    }
    if device.removable {
      return Err(IoError::invalid_input("QMP", "Removable disks cannot grow"));
    }
    validate_disk_growth(block, size)?;
    if size == block.image.virtual_size {
      return Ok(());
    }
    self
      .mutate("block_resize", json!({"node-name": node, "size": size}))
      .await
  }

  async fn query<T: DeserializeOwned>(&mut self, command: &str) -> IoResult<T> {
    let response = self.execute(command).await?;
    serde_json::from_value(response).map_err(|_| {
      self.failed = true;
      invalid("Malformed resource inventory response")
    })
  }

  async fn mutate(&mut self, command: &str, arguments: Value) -> IoResult<()> {
    let response = self.execute_with_args(command, Some(arguments)).await?;
    if !response.is_object() {
      self.failed = true;
      return Err(invalid("Malformed resource mutation response"));
    }
    Ok(())
  }

  async fn execute(&mut self, command: &str) -> IoResult<Value> {
    self.execute_with_args(command, None).await
  }

  async fn execute_with_args(
    &mut self,
    command: &str,
    arguments: Option<Value>,
  ) -> IoResult<Value> {
    if self.failed {
      return Err(invalid("Session is unusable after an interrupted command"));
    }
    if !matches!(
      command,
      "qmp_capabilities"
        | "query-status"
        | "system_powerdown"
        | "query-hotpluggable-cpus"
        | "query-cpus-fast"
        | "query-memory-size-summary"
        | "query-memory-devices"
        | "query-block"
        | "device_add"
        | "object-add"
        | "object-del"
        | "block_resize"
    ) {
      return Err(IoError::invalid_input("QMP", "Unsupported command"));
    }
    // Poison before the first await, including if our caller cancels this future.
    self.failed = true;
    let id = self.next_id;
    self.next_id = id
      .checked_add(1)
      .ok_or_else(|| invalid("Request identifier exhausted"))?;
    let mut request = json!({
      "execute": command,
      "id": id,
    });
    if let Some(arguments) = arguments {
      request["arguments"] = arguments;
    }
    let mut request = serde_json::to_vec(&request)?;
    request.extend_from_slice(b"\r\n");
    if request.len() > MAX_MESSAGE_BYTES {
      return Err(invalid("Request exceeds 64 KiB"));
    }
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

  fn empty_cpu_slot() -> QmpCpuSlot {
    QmpCpuSlot {
      driver: "host-x86_64-cpu".to_owned(),
      props: [("socket-id".to_owned(), 1), ("core-id".to_owned(), 0)]
        .into_iter()
        .collect(),
      vcpus_count: 1,
      qom_path: None,
    }
  }

  fn block_inventory() -> Value {
    json!([{
      "device": "ide0-hd0", "qdev": "disk0", "removable": false,
      "inserted": {
        "node-name": "#block1", "file": "/disks/root.qcow2",
        "ro": false, "drv": "qcow2", "encrypted": false,
        "backing_file_depth": 0,
        "image": {"filename": "/disks/root.qcow2", "format": "qcow2",
          "virtual-size": 1024, "format-specific": {
            "type": "qcow2", "data": {"compat": "1.1", "corrupt": false}
          }}
      }
    }])
  }

  #[ntex::test]
  async fn qmp_parses_resource_inventories() {
    let (stream, peer) = duplex(4096);
    let client = async {
      let mut client = QmpClient::handshake(stream, deadline()).await.unwrap();
      let slots = client.query_cpu_slots().await.unwrap();
      assert_eq!(slots[0].props["socket-id"], 0);
      assert!(slots[0].qom_path.is_some());
      assert_eq!(client.query_cpus_count().await.unwrap(), 2);
      let memory = client.query_memory_size().await.unwrap();
      assert_eq!(memory.base_memory, 1024);
      assert_eq!(memory.plugged_memory, Some(512));
      let devices = client.query_memory_devices().await.unwrap();
      assert_eq!(devices[0].kind, "dimm");
      assert_eq!(devices[0].data.id.as_deref(), Some("nanocl-dimm-1"));
      assert_eq!(devices[0].data.memdev, "/objects/nanocl-mem-1");
      assert_eq!(devices[0].data.size, 512);
      assert_eq!(devices[0].data.slot, Some(0));
      let blocks = client.query_block_devices().await.unwrap();
      assert_eq!(blocks[0].device, "ide0-hd0");
      assert_eq!(blocks[0].qdev.as_deref(), Some("disk0"));
      let node = blocks[0].inserted.as_ref().unwrap();
      assert_eq!(node.node_name, "#block1");
      assert_eq!(node.image.virtual_size, 1024);
      assert_eq!(node.file, node.image.filename);
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      for (command, response) in [
        (
          "query-hotpluggable-cpus",
          json!([{"type": "host-x86_64-cpu", "props": {"socket-id": 0}, "vcpus-count": 1, "qom-path": "/machine/cpu[0]"}]),
        ),
        (
          "query-cpus-fast",
          json!([{"cpu-index": 0}, {"cpu-index": 1}]),
        ),
        (
          "query-memory-size-summary",
          json!({"base-memory": 1024, "plugged-memory": 512}),
        ),
        (
          "query-memory-devices",
          json!([{"type": "dimm", "data": {"id": "nanocl-dimm-1", "size": 512, "slot": 0, "memdev": "/objects/nanocl-mem-1"}}]),
        ),
        ("query-block", block_inventory()),
      ] {
        let request = receive(&mut peer).await;
        assert_eq!(request["execute"], command);
        assert!(request.get("arguments").is_none());
        send(&mut peer, json!({"return": response, "id": request["id"]})).await;
      }
    };
    futures::join!(client, server);
  }

  #[ntex::test]
  async fn qmp_sends_typed_growth_commands() {
    let (stream, peer) = duplex(4096);
    let client = async {
      let mut client = QmpClient::handshake(stream, deadline()).await.unwrap();
      client
        .add_cpu("nanocl-cpu-1", &empty_cpu_slot())
        .await
        .unwrap();
      client
        .add_memory_backend("nanocl-mem-1", 128 * 1024 * 1024)
        .await
        .unwrap();
      client
        .add_memory_device("nanocl-dimm-1", "nanocl-mem-1")
        .await
        .unwrap();
      client
        .delete_memory_backend("nanocl-mem-unused")
        .await
        .unwrap();
      client.resize_block("#block1", 2048).await.unwrap();
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      for (command, arguments) in [
        (
          "device_add",
          json!({"driver": "host-x86_64-cpu", "id": "nanocl-cpu-1", "socket-id": 1, "core-id": 0}),
        ),
        (
          "object-add",
          json!({"qom-type": "memory-backend-ram", "id": "nanocl-mem-1", "size": 128 * 1024 * 1024}),
        ),
        (
          "device_add",
          json!({"driver": "pc-dimm", "id": "nanocl-dimm-1", "memdev": "nanocl-mem-1"}),
        ),
        ("object-del", json!({"id": "nanocl-mem-unused"})),
        ("query-block", Value::Null),
        (
          "block_resize",
          json!({"node-name": "#block1", "size": 2048}),
        ),
      ] {
        let request = receive(&mut peer).await;
        assert_eq!(request["execute"], command);
        assert_eq!(request["arguments"], arguments);
        let response = if command == "query-block" {
          block_inventory()
        } else {
          json!({})
        };
        send(&mut peer, json!({"return": response, "id": request["id"]})).await;
      }
    };
    futures::join!(client, server);
  }

  #[ntex::test]
  async fn qmp_rejects_unmanaged_growth_arguments_without_writing() {
    let (stream, peer) = duplex(1024);
    let client = async {
      let mut client = QmpClient::handshake(stream, deadline()).await.unwrap();
      let mut slot = empty_cpu_slot();
      assert!(client.add_cpu("foreign-cpu", &slot).await.is_err());
      slot.props.insert("driver".to_owned(), 1);
      assert!(client.add_cpu("nanocl-cpu-1", &slot).await.is_err());
      slot = empty_cpu_slot();
      slot.qom_path = Some("/machine/cpu[1]".to_owned());
      assert!(client.add_cpu("nanocl-cpu-1", &slot).await.is_err());
      assert!(client.add_memory_backend("nanocl-mem-1", 0).await.is_err());
      assert!(
        client
          .add_memory_device("nanocl-dimm-1", "foreign-backend")
          .await
          .is_err()
      );
      assert!(
        client
          .delete_memory_backend("foreign-backend")
          .await
          .is_err()
      );
      assert!(client.resize_block("#block1", u64::MAX).await.is_err());
      client.powerdown().await.unwrap();
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      let request = receive(&mut peer).await;
      assert_eq!(request["execute"], "system_powerdown");
      send(&mut peer, json!({"return": {}, "id": request["id"]})).await;
    };
    futures::join!(client, server);
  }

  #[ntex::test]
  async fn qmp_rejects_unsafe_disk_growth() {
    let mut inventories = vec![];
    for (pointer, value) in [
      ("/0/removable", json!(true)),
      ("/0/inserted/ro", json!(true)),
      ("/0/inserted/encrypted", json!(true)),
      ("/0/inserted/backing_file_depth", json!(1)),
      ("/0/inserted/image/virtual-size", json!(4096)),
      (
        "/0/inserted/image/format-specific/data/corrupt",
        json!(true),
      ),
    ] {
      let mut blocks = block_inventory();
      *blocks.pointer_mut(pointer).unwrap() = value;
      inventories.push(blocks);
    }
    for (key, value) in [
      ("backing-filename", json!("parent.qcow2")),
      ("snapshots", json!([{"id": "snapshot-1"}])),
    ] {
      let mut blocks = block_inventory();
      blocks[0]["inserted"]["image"][key] = value;
      inventories.push(blocks);
    }
    let mut blocks = block_inventory();
    blocks[0]["inserted"]["image"]["format-specific"]["data"]["data-file"] =
      json!("external.data");
    inventories.push(blocks);
    let mut blocks = block_inventory();
    let duplicate = blocks[0].clone();
    blocks.as_array_mut().unwrap().push(duplicate);
    inventories.push(blocks);
    inventories.push(json!([]));
    for inventory in inventories {
      let (stream, peer) = duplex(4096);
      let client = async {
        let mut client =
          QmpClient::handshake(stream, deadline()).await.unwrap();
        assert!(client.resize_block("#block1", 2048).await.is_err());
        client.powerdown().await.unwrap();
      };
      let server = async {
        let mut peer = BufReader::new(peer);
        greet(&mut peer).await;
        let request = receive(&mut peer).await;
        assert_eq!(request["execute"], "query-block");
        send(&mut peer, json!({"return": inventory, "id": request["id"]}))
          .await;
        let request = receive(&mut peer).await;
        assert_eq!(request["execute"], "system_powerdown");
        send(&mut peer, json!({"return": {}, "id": request["id"]})).await;
      };
      futures::join!(client, server);
    }
  }

  #[ntex::test]
  async fn qmp_rejects_malformed_resource_responses() {
    for response in [json!({}), json!([{"cpu-index": 0}, {"cpu-index": 0}])] {
      let (stream, peer) = duplex(1024);
      let client = async {
        let mut client =
          QmpClient::handshake(stream, deadline()).await.unwrap();
        assert!(client.query_cpus_count().await.is_err());
        assert!(client.query_status().await.is_err());
      };
      let server = async {
        let mut peer = BufReader::new(peer);
        greet(&mut peer).await;
        let request = receive(&mut peer).await;
        send(&mut peer, json!({"return": response, "id": request["id"]})).await;
      };
      futures::join!(client, server);
    }
  }

  #[ntex::test]
  async fn qmp_cancelled_device_add_requires_fresh_session() {
    let (stream, peer) = duplex(1024);
    let (observed, received) = tokio::sync::oneshot::channel();
    let client = async {
      let mut client = QmpClient::handshake(stream, deadline()).await.unwrap();
      client
        .add_memory_backend("nanocl-mem-1", 128 * 1024 * 1024)
        .await
        .unwrap();
      let mutation =
        Box::pin(client.add_memory_device("nanocl-dimm-1", "nanocl-mem-1"));
      match futures::future::select(mutation, received).await {
        futures::future::Either::Right((Ok(()), pending)) => drop(pending),
        _ => panic!("Mutation must remain unacknowledged"),
      }
      assert!(client.delete_memory_backend("nanocl-mem-1").await.is_err());
    };
    let server = async {
      let mut peer = BufReader::new(peer);
      greet(&mut peer).await;
      let request = receive(&mut peer).await;
      assert_eq!(request["execute"], "object-add");
      send(&mut peer, json!({"return": {}, "id": request["id"]})).await;
      let request = receive(&mut peer).await;
      assert_eq!(request["execute"], "device_add");
      observed.send(()).unwrap();
      let mut line = String::new();
      assert_eq!(peer.read_line(&mut line).await.unwrap(), 0);
    };
    futures::join!(client, server);
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
