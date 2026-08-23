use std::{cell::RefCell, io, rc::Rc, time::Instant};

use bollard_next::{
  container::LogOutput,
  exec::{ResizeExecOptions, StartExecOptions, StartExecResults},
};
use futures::{StreamExt, future::ready};
use ntex::{
  Service, chain,
  channel::{mpsc, oneshot},
  fn_service, rt,
  service::{fn_factory_with_config, fn_shutdown, map_config},
  util::Bytes,
  web, ws,
};
use tokio::io::AsyncWriteExt;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::{
  ProcessExecCreateOptions, ProcessExecCreated, ProcessExecInputControl,
  ProcessExecOutputChannel, ProcessExecOutputControl,
};

use crate::{
  models::{SystemState, WsConState},
  utils,
};

#[derive(Debug, PartialEq)]
enum BridgeCommand {
  Stdin(Bytes),
  Control(ProcessExecInputControl),
  ProtocolError(String),
  ClientClosed,
}

fn decode_exec_input_frame(frame: ws::Frame) -> Result<BridgeCommand, String> {
  match frame {
    ws::Frame::Binary(bytes) => Ok(BridgeCommand::Stdin(bytes)),
    ws::Frame::Text(bytes) => serde_json::from_slice(&bytes)
      .map(BridgeCommand::Control)
      .map_err(|err| format!("invalid exec control: {err}")),
    _ => Err("unsupported exec WebSocket frame".to_owned()),
  }
}

fn validate_bridge_command(
  command: &BridgeCommand,
  tty: bool,
  stdin_open: bool,
) -> Result<(), &'static str> {
  match command {
    BridgeCommand::Stdin(_)
    | BridgeCommand::Control(ProcessExecInputControl::StdinEof)
      if !stdin_open =>
    {
      Err("stdin is not enabled for this exec")
    }
    BridgeCommand::Control(ProcessExecInputControl::Resize {
      width,
      height,
    }) if !tty => Err("resize is only valid for a TTY exec"),
    BridgeCommand::Control(ProcessExecInputControl::Resize {
      width,
      height,
    }) if *width == 0 || *height == 0 => {
      Err("resize width and height must be non-zero")
    }
    _ => Ok(()),
  }
}

fn exec_output_frame(output: LogOutput) -> Option<Bytes> {
  let (channel, message) = match output {
    LogOutput::StdOut { message } => {
      (ProcessExecOutputChannel::Stdout, message)
    }
    LogOutput::StdErr { message } => {
      (ProcessExecOutputChannel::Stderr, message)
    }
    LogOutput::Console { message } => {
      (ProcessExecOutputChannel::Console, message)
    }
    LogOutput::StdIn { .. } => return None,
  };
  let mut frame = Vec::with_capacity(message.len() + 1);
  frame.push(channel.marker());
  frame.extend_from_slice(&message);
  Some(Bytes::from(frame))
}

fn limited_error_message(error: impl ToString) -> String {
  error.to_string().chars().take(512).collect()
}

async fn send_control(
  sink: &ws::WsSink,
  control: ProcessExecOutputControl,
) -> Result<(), String> {
  let message = serde_json::to_string(&control)
    .map_err(|err| limited_error_message(err.to_string()))?;
  sink
    .send(ws::Message::Text(message.into()))
    .await
    .map_err(limited_error_message)
}

async fn close_websocket(
  sink: &ws::WsSink,
  code: ws::CloseCode,
  description: &'static str,
) {
  let _ = sink
    .send(ws::Message::Close(Some((code, description).into())))
    .await;
}

async fn fail_bridge(
  sink: &ws::WsSink,
  message: impl ToString,
  close_code: ws::CloseCode,
) {
  let message = limited_error_message(message);
  log::error!("Process exec bridge failed: {message}");
  let _ = send_control(
    sink,
    ProcessExecOutputControl::Error {
      message: message.clone(),
    },
  )
  .await;
  close_websocket(sink, close_code, "process exec bridge failed").await;
}

async fn bridge_process_exec(
  docker: bollard_next::Docker,
  id: String,
  tty: bool,
  mut stdin_open: bool,
  sink: ws::WsSink,
  commands: mpsc::Receiver<BridgeCommand>,
) {
  let result = match docker
    .start_exec(
      &id,
      Some(StartExecOptions {
        detach: false,
        tty,
        output_capacity: None,
      }),
    )
    .await
  {
    Ok(result) => result,
    Err(err) => {
      fail_bridge(&sink, err, ws::CloseCode::Error).await;
      return;
    }
  };
  let StartExecResults::Attached { output, mut input } = result else {
    fail_bridge(
      &sink,
      "Docker returned a detached stream for an attached exec",
      ws::CloseCode::Error,
    )
    .await;
    return;
  };
  let mut output = output.fuse();
  let mut commands = commands.fuse();

  loop {
    futures::select! {
      output = output.next() => match output {
        Some(Ok(output)) => {
          if let Some(frame) = exec_output_frame(output)
            && sink.send(ws::Message::Binary(frame)).await.is_err()
          {
            return;
          }
        }
        Some(Err(err)) => {
          fail_bridge(&sink, err, ws::CloseCode::Error).await;
          return;
        }
        None => {
          let inspect = match docker.inspect_exec(&id).await {
            Ok(inspect) => inspect,
            Err(err) => {
              fail_bridge(&sink, err, ws::CloseCode::Error).await;
              return;
            }
          };
          let Some(code) = inspect.exit_code else {
            fail_bridge(
              &sink,
              "Docker did not report a final exec exit code",
              ws::CloseCode::Error,
            )
            .await;
            return;
          };
          if send_control(
            &sink,
            ProcessExecOutputControl::Exit { code },
          )
          .await
          .is_ok()
          {
            close_websocket(&sink, ws::CloseCode::Normal, "exec complete")
              .await;
          }
          return;
        }
      },
      command = commands.next() => {
        let Some(command) = command else {
          // Losing the Nanocl attachment does not stop the Docker exec.
          return;
        };
        if let BridgeCommand::ProtocolError(message) = command {
          fail_bridge(&sink, message, ws::CloseCode::Protocol).await;
          return;
        }
        if let Err(message) =
          validate_bridge_command(&command, tty, stdin_open)
        {
          fail_bridge(&sink, message, ws::CloseCode::Policy).await;
          return;
        }
        match command {
          BridgeCommand::Stdin(bytes) => {
            if let Err(err) = input.write_all(&bytes).await {
              fail_bridge(&sink, err, ws::CloseCode::Error).await;
              return;
            }
          }
          BridgeCommand::Control(ProcessExecInputControl::StdinEof) => {
            if let Err(err) = input.shutdown().await {
              fail_bridge(&sink, err, ws::CloseCode::Error).await;
              return;
            }
            stdin_open = false;
          }
          BridgeCommand::Control(ProcessExecInputControl::Resize {
            width,
            height,
          }) => {
            if let Err(err) = docker
              .resize_exec(&id, ResizeExecOptions { width, height })
              .await
            {
              fail_bridge(&sink, err, ws::CloseCode::Error).await;
              return;
            }
          }
          BridgeCommand::Control(ProcessExecInputControl::Detach) => {
            let _ = send_control(
              &sink,
              ProcessExecOutputControl::Detached,
            )
            .await;
            close_websocket(&sink, ws::CloseCode::Normal, "detached").await;
            return;
          }
          BridgeCommand::ClientClosed => return,
          BridgeCommand::ProtocolError(_) => unreachable!(),
        }
      }
    }
  }
}

async fn ws_exec_service(
  (id, tty, stdin_open, sink, state): (
    String,
    bool,
    bool,
    ws::WsSink,
    web::types::State<SystemState>,
  ),
) -> Result<
  impl Service<ws::Frame, Response = Option<ws::Message>, Error = io::Error>,
  web::Error,
> {
  let con_state = Rc::new(RefCell::new(WsConState::new()));
  let (heartbeat_tx, heartbeat_rx) = oneshot::channel();
  rt::spawn(utils::ws::heartbeat(
    con_state.clone(),
    sink.clone(),
    heartbeat_rx,
  ));
  let (command_tx, command_rx) = mpsc::channel();
  rt::spawn(bridge_process_exec(
    state.inner.docker_api.clone(),
    id,
    tty,
    stdin_open,
    sink,
    command_rx,
  ));

  let service = fn_service(move |frame| {
    let response = match frame {
      ws::Frame::Ping(message) => {
        con_state.borrow_mut().hb = Instant::now();
        Some(ws::Message::Pong(message))
      }
      ws::Frame::Pong(_) => {
        con_state.borrow_mut().hb = Instant::now();
        None
      }
      frame @ (ws::Frame::Text(_) | ws::Frame::Binary(_)) => {
        let command = decode_exec_input_frame(frame)
          .unwrap_or_else(BridgeCommand::ProtocolError);
        let _ = command_tx.send(command);
        None
      }
      ws::Frame::Close(reason) => {
        let _ = command_tx.send(BridgeCommand::ClientClosed);
        Some(ws::Message::Close(reason))
      }
      _ => {
        let _ = command_tx.send(BridgeCommand::ProtocolError(
          "unsupported exec WebSocket frame".to_owned(),
        ));
        None
      }
    };
    ready(Ok(response))
  });
  let on_shutdown = fn_shutdown(async move || {
    let _ = heartbeat_tx.send(());
  });
  Ok(chain(service).and_then(on_shutdown))
}

/// Create an exec instance for a concrete process name or ID
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{name}/exec",
  request_body = ProcessExecCreateOptions,
  params(
    ("name" = String, Path, description = "Name or id of the container"),
  ),
  responses(
    (status = 201, description = "Process exec created", body = ProcessExecCreated),
    (status = 404, description = "Process doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::post("/processes/{name}/exec")]
pub async fn create_process_exec(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ProcessExecCreateOptions>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  let result = state
    .inner
    .docker_api
    .create_exec(&name, payload.into_inner().into())
    .await?;
  Ok(web::HttpResponse::Created().json(&ProcessExecCreated::from(result)))
}

/// Start a Docker exec instance without attaching streams
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/exec/{id}/start",
  params(
    ("id" = String, Path, description = "Docker exec instance ID"),
  ),
  responses(
    (status = 200, description = "Process exec started detached"),
    (status = 404, description = "Process exec doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn start_process_exec_detached(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, id) = path.into_inner();
  let inspect = state.inner.docker_api.inspect_exec(&id).await?;
  let tty = inspect
    .process_config
    .and_then(|config| config.tty)
    .unwrap_or(false);
  let result = state
    .inner
    .docker_api
    .start_exec(
      &id,
      Some(StartExecOptions {
        detach: true,
        tty,
        output_capacity: None,
      }),
    )
    .await?;
  match result {
    StartExecResults::Detached => Ok(web::HttpResponse::Ok().finish()),
    StartExecResults::Attached { .. } => Err(HttpError::internal_server_error(
      "Docker returned an attached stream for a detached exec",
    )),
  }
}

/// Start and attach to a Docker exec instance through an RFC6455 WebSocket
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/exec/{id}/start",
  description = "Binary client frames are raw stdin. Text controls are stdin_eof, resize, and detach. Server binary frames begin with 0x01 stdout, 0x02 stderr, or 0x03 console; text controls report exit, detached, or error.",
  params(
    ("id" = String, Path, description = "Docker exec instance ID"),
  ),
  responses(
    (status = 101, description = "Attached process exec WebSocket"),
    (status = 404, description = "Process exec doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn start_process_exec_attached(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  req: web::HttpRequest,
) -> Result<web::HttpResponse, web::Error> {
  let (_, id) = path.into_inner();
  let inspect = state
    .inner
    .docker_api
    .inspect_exec(&id)
    .await
    .map_err(HttpError::from)?;
  let tty = inspect
    .process_config
    .and_then(|config| config.tty)
    .unwrap_or(false);
  let stdin_open = inspect.open_stdin.unwrap_or(false);

  web::ws::start(
    req,
    None::<&str>,
    map_config(fn_factory_with_config(ws_exec_service), move |sink| {
      (id.clone(), tty, stdin_open, sink, state.clone())
    }),
  )
  .await
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn process_exec_binary_stdin_remains_exact() {
    let bytes = Bytes::from_static(&[0x00, 0xff, b'a', 0x80]);
    let command =
      decode_exec_input_frame(ws::Frame::Binary(bytes.clone())).unwrap();

    assert_eq!(command, BridgeCommand::Stdin(bytes));
  }

  #[test]
  fn process_exec_output_markers_preserve_raw_bytes() {
    let stdout = exec_output_frame(LogOutput::StdOut {
      message: vec![0x00, 0xff].into(),
    })
    .unwrap();
    let stderr = exec_output_frame(LogOutput::StdErr {
      message: vec![0x80, b'e'].into(),
    })
    .unwrap();
    let console = exec_output_frame(LogOutput::Console {
      message: vec![b't', 0x00].into(),
    })
    .unwrap();

    assert_eq!(stdout.as_ref(), &[0x01, 0x00, 0xff]);
    assert_eq!(stderr.as_ref(), &[0x02, 0x80, b'e']);
    assert_eq!(console.as_ref(), &[0x03, b't', 0x00]);
  }

  #[test]
  fn process_exec_rejects_stdin_when_disabled() {
    let command = BridgeCommand::Stdin(Bytes::from_static(b"input"));
    let eof = BridgeCommand::Control(ProcessExecInputControl::StdinEof);

    assert_eq!(
      validate_bridge_command(&command, false, false),
      Err("stdin is not enabled for this exec")
    );
    assert_eq!(
      validate_bridge_command(&eof, false, false),
      Err("stdin is not enabled for this exec")
    );
    assert!(validate_bridge_command(&command, false, true).is_ok());
    assert!(validate_bridge_command(&eof, false, true).is_ok());
  }

  #[test]
  fn process_exec_validates_resize() {
    let resize = BridgeCommand::Control(ProcessExecInputControl::Resize {
      width: 80,
      height: 24,
    });
    let zero = BridgeCommand::Control(ProcessExecInputControl::Resize {
      width: 0,
      height: 24,
    });

    assert_eq!(
      validate_bridge_command(&resize, false, true),
      Err("resize is only valid for a TTY exec")
    );
    assert_eq!(
      validate_bridge_command(&zero, true, true),
      Err("resize width and height must be non-zero")
    );
    assert!(validate_bridge_command(&resize, true, true).is_ok());
  }
}
