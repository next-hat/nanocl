use std::{cell::RefCell, io, rc::Rc, time::Instant};

use bollard_next::service::ContainerInspectResponse;
use futures::future::{Either, select};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  process::{Process, ProcessKind},
  vm::Vm,
};
use ntex::{Service, channel::oneshot, fn_service, rt, util::Bytes, web, ws};
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  net::UnixStream,
  sync::mpsc,
};

use crate::{
  models::{ProcessDb, SystemState, VmDb, WsConState},
  repositories::generic::*,
  utils,
};

const MAX_FRAME_SIZE: usize = 65_536;

fn validate_runtime(
  vm: &Vm,
  process: &Process,
  runtime: &ContainerInspectResponse,
  node: &str,
) -> HttpResult<()> {
  let expected_name = format!("{}.v", vm.spec.vm_key);
  let labels = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  let label = |name: &str| {
    labels
      .and_then(|labels| labels.get(name))
      .map(String::as_str)
  };
  let generation = vm.spec.key.to_string();
  if process.kind != ProcessKind::Vm
    || process.kind_key != vm.spec.vm_key
    || process.name != expected_name
    || process.node_name != node
    || runtime.id.as_deref() != Some(process.key.as_str())
    || runtime.name.as_deref() != Some(format!("/{expected_name}").as_str())
    || label("io.nanocl.v") != Some(vm.spec.vm_key.as_str())
    || label("io.nanocl.n") != Some(vm.namespace_name.as_str())
    || label("io.nanocl.vm-spec") != Some(generation.as_str())
    || label("io.nanocl.vm-console") != Some("true")
    || label("io.nanocl.init-v").is_some()
  {
    return Err(HttpError::conflict(
      "VM console runtime identity does not match the current VM",
    ));
  }
  if runtime.state.as_ref().and_then(|state| state.running) != Some(true) {
    return Err(HttpError::conflict("VM runtime is not running"));
  }
  Ok(())
}

fn console_input(frame: ws::Frame) -> HttpResult<Bytes> {
  let bytes = match frame {
    ws::Frame::Binary(bytes)
    | ws::Frame::Continuation(ws::Item::FirstBinary(bytes))
    | ws::Frame::Continuation(ws::Item::Continue(bytes))
    | ws::Frame::Continuation(ws::Item::Last(bytes)) => bytes,
    _ => {
      return Err(HttpError::bad_request(
        "Console accepts binary RFB data only",
      ));
    }
  };
  if bytes.len() > MAX_FRAME_SIZE {
    return Err(HttpError::bad_request("Console frame exceeds 64 KiB"));
  }
  Ok(bytes)
}

async fn relay_console<S, F, Fut>(
  mut stream: S,
  mut input: mpsc::Receiver<Bytes>,
  send: F,
) -> io::Result<()>
where
  S: AsyncRead + AsyncWrite + Unpin,
  F: Fn(Bytes) -> Fut,
  Fut: std::future::Future<Output = io::Result<()>>,
{
  let mut buffer = [0u8; 16 * 1024];
  loop {
    let next = {
      let read = stream.read(&mut buffer);
      let receive = input.recv();
      futures::pin_mut!(read, receive);
      match select(read, receive).await {
        Either::Left((result, _)) => Either::Left(result),
        Either::Right((message, _)) => Either::Right(message),
      }
    };
    match next {
      Either::Left(result) => {
        let count = result?;
        if count == 0 {
          return Ok(());
        }
        send(Bytes::copy_from_slice(&buffer[..count])).await?;
      }
      Either::Right(message) => {
        let Some(message) = message else {
          return Ok(());
        };
        stream.write_all(&message).await?;
      }
    }
  }
}

async fn cancellable_relay<S, F, Fut>(
  stream: S,
  input: mpsc::Receiver<Bytes>,
  send: F,
  stop: oneshot::Receiver<()>,
) -> Option<io::Result<()>>
where
  S: AsyncRead + AsyncWrite + Unpin,
  F: Fn(Bytes) -> Fut,
  Fut: std::future::Future<Output = io::Result<()>>,
{
  let relay = relay_console(stream, input, send);
  futures::pin_mut!(stop, relay);
  match select(stop, relay).await {
    Either::Left(_) => None,
    Either::Right((result, _)) => Some(result),
  }
}

fn console_service(
  stream: UnixStream,
) -> impl Service<ws::WsSink, ws::Frame, Res = Option<ws::Message>, Error = io::Error>
{
  let con_state = Rc::new(RefCell::new(WsConState::new()));
  let heartbeat_state = con_state.clone();
  let (heartbeat_tx, heartbeat_rx) = oneshot::channel();
  let (stop_tx, stop_rx) = oneshot::channel();
  let stop_tx = Rc::new(RefCell::new(Some(stop_tx)));
  let stop_on_shutdown = stop_tx.clone();
  // Backpressure bounds queued input to at most eight 64 KiB frames.
  let (input_tx, input_rx) = mpsc::channel(8);
  let startup = Rc::new(RefCell::new(Some((
    stream,
    input_rx,
    heartbeat_rx,
    stop_rx,
  ))));
  let service = fn_service(move |frame| {
    let con_state = con_state.clone();
    let input_tx = input_tx.clone();
    let stop_tx = stop_tx.clone();
    async move {
      match frame {
        ws::Frame::Ping(bytes) => {
          con_state.borrow_mut().hb = Instant::now();
          Ok(Some(ws::Message::Pong(bytes)))
        }
        ws::Frame::Pong(_) => {
          con_state.borrow_mut().hb = Instant::now();
          Ok(None)
        }
        ws::Frame::Close(reason) => {
          if let Some(stop) = stop_tx.borrow_mut().take() {
            let _ = stop.send(());
          }
          Ok(Some(ws::Message::Close(reason)))
        }
        frame => match console_input(frame) {
          Ok(bytes) => {
            if input_tx.try_send(bytes).is_err() {
              if let Some(stop) = stop_tx.borrow_mut().take() {
                let _ = stop.send(());
              }
              Ok(Some(ws::Message::Close(Some(
                (
                  ws::CloseCode::Policy,
                  "Console input queue is full or closed",
                )
                  .into(),
              ))))
            } else {
              Ok(None)
            }
          }
          Err(_) => {
            if let Some(stop) = stop_tx.borrow_mut().take() {
              let _ = stop.send(());
            }
            Ok(Some(ws::Message::Close(Some(
              (ws::CloseCode::Unsupported, "Expected binary RFB data").into(),
            ))))
          }
        },
      }
    }
  });
  ntex::service(service)
    .readiness(async move |sink: &ws::WsSink| {
      if let Some((stream, input, heartbeat_rx, stop_rx)) =
        startup.borrow_mut().take()
      {
        rt::spawn(utils::ws::heartbeat(
          heartbeat_state.clone(),
          sink.clone(),
          heartbeat_rx,
        ));
        let sink = sink.clone();
        rt::spawn(async move {
          // Cancellation covers socket writes and WebSocket backpressure too.
          let output = sink.clone();
          let result = cancellable_relay(
            stream,
            input,
            move |bytes| {
              let output = output.clone();
              async move {
                output
                  .send(ws::Message::Binary(bytes))
                  .await
                  .map_err(io::Error::other)
              }
            },
            stop_rx,
          )
          .await;
          if let Some(Err(error)) = result {
            log::debug!("VM console disconnected: {error}");
          }
          sink.io().close();
        });
      }
      Ok(())
    })
    .shutdown(async move |_| {
      let _ = heartbeat_tx.send(());
      if let Some(stop) = stop_on_shutdown.borrow_mut().take() {
        let _ = stop.send(());
      }
    })
}

/// Open a VM's graphical console using binary RFB over WebSocket.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{key}/console",
  description = "Requires Console: true and a current local VM runtime. Access follows the daemon's configured operator access controls. Binary frames contain unmodified RFB bytes, limited to 64 KiB per frame. There is no per-user VM ownership model.",
  params(("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format")),
  responses(
    (status = 101, description = "Binary RFB console WebSocket"),
    (status = 403, description = "Console is disabled"),
    (status = 404, description = "VM does not exist"),
    (status = 409, description = "VM runtime is not ready or does not match")
  )
))]
pub async fn console_vm(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  req: web::HttpRequest,
) -> HttpResult<web::HttpResponse> {
  if let Err(error) = ws::verify_handshake(req.head()) {
    return utils::ws::response(Err(error.into()));
  }
  let key = utils::key::parse_resource_key(&path.1)?;
  let vm = VmDb::transform_read_by_pk(key.as_str(), &state.inner.pool).await?;
  if !vm.spec.console.unwrap_or(false) {
    return Err(HttpError::forbidden("VM console is disabled"));
  }
  let processes =
    ProcessDb::read_by_kind_key(key.as_str(), None, &state.inner.pool).await?;
  let expected_name = format!("{}.v", vm.spec.vm_key);
  let mut candidates = processes
    .iter()
    .filter(|process| process.name == expected_name);
  let process = candidates
    .next()
    .ok_or_else(|| HttpError::conflict("VM runtime is not ready"))?;
  if candidates.next().is_some() {
    return Err(HttpError::conflict("VM runtime identity is ambiguous"));
  }
  let runtime = state
    .inner
    .docker_api
    .inspect_container(&process.key, None)
    .await?;
  let mut boot_vm = vm.clone();
  boot_vm.spec = utils::container::vm::validate_boot_spec(&vm, &runtime, state)
    .await
    .map_err(super::lifecycle::lifecycle_error)?;
  validate_runtime(&boot_vm, process, &runtime, &state.inner.config.hostname)?;
  let socket = utils::container::vm::socket_directory(
    &vm,
    &runtime,
    &state.inner.config.state_dir,
  )
  .map_err(super::lifecycle::lifecycle_error)?
  .join("console.sock");
  let stream = UnixStream::connect(socket)
    .await
    .map_err(|_| HttpError::conflict("VM console is not ready"))?;
  let protocol =
    web::ws::subprotocols(&req).find(|protocol| *protocol == "binary");
  utils::ws::response(
    web::ws::start(&req, protocol, console_service(stream)).await,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn console_http_requests_reach_vm_key_validation() {
    use diesel::{PgConnection, r2d2::ConnectionManager};
    use std::sync::Arc;

    use crate::models::{Pool, SystemStateInner};

    // No connection or event loop is started. The malformed key must be
    // rejected before the handler reaches its database or Docker clients.
    let pool = Pool::builder()
      .min_idle(Some(0))
      .max_size(1)
      .build_unchecked(ConnectionManager::<PgConnection>::new(
        "invalid-test-connection-string",
      ));
    let (event_emitter, _events) = futures::channel::mpsc::unbounded();
    let state = SystemState {
      inner: Arc::new(SystemStateInner {
        pool,
        docker_api: bollard_next::Docker::connect_with_unix(
          "/dev/null",
          1,
          bollard_next::API_DEFAULT_VERSION,
        )
        .unwrap(),
        config: nanocl_stubs::config::DaemonConfig {
          hosts: vec![],
          state_dir: String::new(),
          docker_host: String::new(),
          store_addr: None,
          gateway: String::new(),
          hostname: "unit-test".into(),
          nodes: vec![],
          advertise_addr: String::new(),
          conf_dir: String::new(),
          gid: 0,
          ssl: None,
        },
        task_manager: Default::default(),
        event_emitter,
        event_emitter_raw: Default::default(),
        arbiter: rt::Arbiter::current(),
      }),
    };
    for origin in [None, Some("https://foreign.example"), Some("null")] {
      let mut request = web::test::TestRequest::default()
        .uri("http://daemon.test/v0.19.0/vms/not-a-canonical-key/console")
        .header("Host", "daemon.test")
        .header("Connection", "upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==");
      if let Some(origin) = origin {
        request = request.header("Origin", origin);
      }
      let request = request.to_http_request();
      assert!(ws::verify_handshake(request.head()).is_ok());
      let error = console_vm(
        &state,
        (),
        web::types::Path::from((
          "v0.19.0".to_owned(),
          "not-a-canonical-key".to_owned(),
        )),
        request,
      )
      .await
      .unwrap_err();
      assert_eq!(error.status, ntex::http::StatusCode::BAD_REQUEST);
    }
    assert_eq!(state.inner.pool.state().connections, 0);
  }

  #[test]
  fn console_binary_input_is_exact_and_bounded() {
    let bytes = Bytes::from_static(&[0, 255, 128, 13, 10]);
    assert_eq!(
      console_input(ws::Frame::Binary(bytes.clone())).unwrap(),
      bytes
    );
    assert!(
      console_input(ws::Frame::Text(Bytes::from_static(b"text"))).is_err()
    );
    assert!(
      console_input(ws::Frame::Continuation(ws::Item::FirstText(Bytes::new())))
        .is_err()
    );
    assert!(
      console_input(ws::Frame::Binary(vec![0; MAX_FRAME_SIZE + 1].into()))
        .is_err()
    );
  }

  #[test]
  fn console_runtime_rejects_wrong_vm_node_generation_and_stopped_runtime() {
    let vm = Vm {
      namespace_name: "global".into(),
      created_at: Default::default(),
      status: Default::default(),
      spec: nanocl_stubs::vm_spec::VmSpec {
        vm_key: "global.desktop".into(),
        ..Default::default()
      },
    };
    let runtime: ContainerInspectResponse = serde_json::from_value(serde_json::json!({
      "Id": "container-id", "Name": "/global.desktop.v", "State": { "Running": true },
      "Config": { "Labels": { "io.nanocl.v": "global.desktop", "io.nanocl.n": "global", "io.nanocl.vm-spec": vm.spec.key.to_string(), "io.nanocl.vm-console": "true" } }
    })).unwrap();
    let process = Process {
      key: "container-id".into(),
      name: "global.desktop.v".into(),
      kind: ProcessKind::Vm,
      kind_key: "global.desktop".into(),
      node_name: "local".into(),
      created_at: Default::default(),
      updated_at: Default::default(),
      ip_address: None,
      data: runtime.clone(),
    };
    assert!(validate_runtime(&vm, &process, &runtime, "local").is_ok());
    assert!(validate_runtime(&vm, &process, &runtime, "other-node").is_err());
    let mut wrong_process = process.clone();
    wrong_process.kind_key = "global.other".into();
    assert!(validate_runtime(&vm, &wrong_process, &runtime, "local").is_err());
    for (label, value) in [
      ("io.nanocl.v", "global.other"),
      ("io.nanocl.n", "other"),
      ("io.nanocl.vm-spec", "old-generation"),
      ("io.nanocl.vm-console", "false"),
      ("io.nanocl.init-v", "true"),
    ] {
      let mut wrong = runtime.clone();
      wrong
        .config
        .as_mut()
        .unwrap()
        .labels
        .as_mut()
        .unwrap()
        .insert(label.into(), value.into());
      assert!(validate_runtime(&vm, &process, &wrong, "local").is_err());
    }
    let mut stopped = runtime.clone();
    stopped.state.as_mut().unwrap().running = Some(false);
    assert!(validate_runtime(&vm, &process, &stopped, "local").is_err());
  }

  #[ntex::test]
  async fn console_disconnect_cancels_blocked_output_and_drops_backend() {
    let (backend, mut guest) = tokio::io::duplex(64);
    let (_input_tx, input_rx) = mpsc::channel(1);
    let (stop_tx, stop_rx) = oneshot::channel();
    let (entered_tx, entered_rx) = oneshot::channel();
    let entered_tx = Rc::new(RefCell::new(Some(entered_tx)));
    let task = rt::spawn(cancellable_relay(
      backend,
      input_rx,
      move |_| {
        if let Some(tx) = entered_tx.borrow_mut().take() {
          let _ = tx.send(());
        }
        std::future::pending::<io::Result<()>>()
      },
      stop_rx,
    ));
    guest.write_all(b"RFB 003.008\n").await.unwrap();
    ntex::time::timeout(std::time::Duration::from_secs(1), entered_rx)
      .await
      .unwrap()
      .unwrap();
    stop_tx.send(()).unwrap();
    assert!(task.await.unwrap().is_none());
    let mut byte = [0];
    assert_eq!(guest.read(&mut byte).await.unwrap(), 0);
  }
}
