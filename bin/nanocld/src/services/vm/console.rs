use std::{cell::RefCell, io, path::PathBuf, rc::Rc, time::Instant};

use bollard_next::service::ContainerInspectResponse;
use futures::future::{Either, select};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  process::{Process, ProcessKind},
  vm::Vm,
};
use ntex::{
  Service, channel::oneshot, fn_service, http::header, rt, util::Bytes, web, ws,
};
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

// Listener tags are assigned by the server, never by request headers.
pub(crate) const CONSOLE_UNIX_TRANSPORT: &str = "nanocld-console-unix";
pub(crate) const CONSOLE_MTLS_TRANSPORT: &str = "nanocld-console-mtls";
const MAX_FRAME_SIZE: usize = 65_536;

fn authorize_transport(
  tag: Option<&str>,
  peer_certificate: bool,
) -> HttpResult<()> {
  match tag {
    Some(CONSOLE_UNIX_TRANSPORT) => Ok(()),
    Some(CONSOLE_MTLS_TRANSPORT) if peer_certificate => Ok(()),
    _ => Err(HttpError::forbidden(
      "VM control requires the daemon Unix socket or mutual TLS",
    )),
  }
}

fn validate_origin(
  origin: Option<&str>,
  host: Option<&str>,
  tls: bool,
) -> HttpResult<()> {
  let Some(origin) = origin else {
    return Ok(());
  };
  // Browsers use mutual TLS directly, or a local authenticated example bridge.
  // Forwarded headers deliberately do not establish trust or an allowed origin.
  let Some(host) = host else {
    return Err(HttpError::forbidden(
      "VM control Origin has no matching Host",
    ));
  };
  if !tls || origin != format!("https://{host}") {
    return Err(HttpError::forbidden(
      "VM control Origin must match the mutual TLS endpoint",
    ));
  }
  Ok(())
}

/// Apply the same daemon-operator boundary to console and lifecycle requests.
pub(crate) fn authorize_request(req: &web::HttpRequest) -> HttpResult<()> {
  let io = req.io();
  let tag = io.map(|io| io.tag());
  let certificate = io.is_some_and(|io| {
    io.query::<ntex::tls::openssl::PeerCert>()
      .as_ref()
      .is_some()
  });
  authorize_transport(tag, certificate)?;
  let origin = req
    .headers()
    .get(header::ORIGIN)
    .map(|value| value.to_str())
    .transpose()
    .map_err(HttpError::bad_request)?;
  let host = req
    .headers()
    .get(header::HOST)
    .and_then(|value| value.to_str().ok());
  validate_origin(origin, host, tag == Some(CONSOLE_MTLS_TRANSPORT))
}

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
  description = "Requires Console: true and a current local VM runtime. Available only over the daemon Unix socket or verified mutual TLS; plaintext TCP is denied. Browser Origin must match the HTTPS endpoint. Binary frames contain unmodified RFB bytes, limited to 64 KiB per frame. Access follows NanoCL's daemon-operator trust model, not per-user VM ownership.",
  params(("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format")),
  responses(
    (status = 101, description = "Binary RFB console WebSocket"),
    (status = 403, description = "Untrusted transport, Origin, or disabled console"),
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
  authorize_request(&req)?;
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
  validate_runtime(&vm, process, &runtime, &state.inner.config.hostname)?;
  let socket = PathBuf::from(&state.inner.config.state_dir)
    .join("run/vms")
    .join(&vm.spec.vm_key)
    .join(vm.spec.key.to_string())
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

  #[test]
  fn vm_control_trust_requires_listener_identity_and_verified_tls() {
    assert!(authorize_transport(Some(CONSOLE_UNIX_TRANSPORT), false).is_ok());
    assert!(authorize_transport(Some(CONSOLE_MTLS_TRANSPORT), true).is_ok());
    for tag in [None, Some("--"), Some("http"), Some(CONSOLE_MTLS_TRANSPORT)] {
      assert!(authorize_transport(tag, false).is_err());
    }
    for tag in [None, Some("--"), Some("http")] {
      let error = authorize_transport(tag, true).unwrap_err();
      assert_eq!(error.status, ntex::http::StatusCode::FORBIDDEN);
    }
  }

  #[test]
  fn vm_control_origin_rejects_cross_site_null_and_plaintext() {
    assert!(validate_origin(None, None, false).is_ok());
    assert!(
      validate_origin(Some("https://host:8443"), Some("host:8443"), true)
        .is_ok()
    );
    for origin in [
      "null",
      "http://host:8443",
      "https://evil",
      "https://host:8443/",
      "https://host:8443.evil",
    ] {
      assert!(validate_origin(Some(origin), Some("host:8443"), true).is_err());
    }
    assert!(
      validate_origin(Some("https://host"), Some("host"), false).is_err()
    );
    assert!(validate_origin(Some("https://host"), None, true).is_err());
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
