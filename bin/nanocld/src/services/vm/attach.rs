use std::{cell::RefCell, io, rc::Rc, time::Instant};

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

use bollard_next::container::AttachContainerOptions;
use nanocl_error::http::HttpError;
use nanocl_stubs::process::OutputLog;

use crate::{
  models::{SystemState, WsConState},
  utils,
};

async fn ws_attach_service(
  (key, sink, state): (String, ws::WsSink, web::types::State<SystemState>),
) -> Result<
  impl Service<ws::Frame, Response = Option<ws::Message>, Error = io::Error>,
  web::Error,
> {
  // start heartbeat task
  let con_state = Rc::new(RefCell::new(WsConState::new()));
  let (tx, rx) = oneshot::channel();
  rt::spawn(utils::ws::heartbeat(con_state.clone(), sink.clone(), rx));
  let (s_cmd, mut r_cmd) = mpsc::channel::<Result<Bytes, web::Error>>();
  let stream = state
    .inner
    .docker_api
    .attach_container(
      &format!("{key}.v"),
      Some(AttachContainerOptions::<String> {
        stdin: Some(true),
        stdout: Some(true),
        stderr: Some(true),
        stream: Some(true),
        logs: Some(false),
        detach_keys: Some("ctrl-c".to_owned()),
      }),
    )
    .await
    .map_err(HttpError::internal_server_error)?;
  rt::spawn(async move {
    let mut output = stream.output;
    while let Some(output) = output.next().await {
      let output = match output {
        Ok(output) => output,
        Err(e) => {
          log::error!("Error reading output from process: {e}");
          break;
        }
      };
      let output_log: OutputLog = output.into();
      let output = serde_json::to_vec(&output_log);
      let mut output = match output {
        Ok(output) => output,
        Err(e) => {
          log::error!("Error serializing output: {e}");
          break;
        }
      };
      output.push(b'\n');
      let msg = ws::Message::Binary(Bytes::from(output));
      if sink.send(msg).await.is_err() {
        break;
      }
    }
  });
  rt::spawn(async move {
    let mut stdin = stream.input;
    while let Some(cmd) = r_cmd.next().await {
      let cmd = match cmd {
        Ok(cmd) => cmd,
        Err(e) => {
          log::error!("Error input for process: {e}");
          break;
        }
      };
      if stdin.write_all(&cmd).await.is_err() {
        break;
      }
    }
  });
  // handler service for incoming websocket frames
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
        let _ = s_cmd.send(Ok(text));
        None
      }
      ws::Frame::Binary(_) => None,
      ws::Frame::Close(reason) => Some(ws::Message::Close(reason)),
      _ => Some(ws::Message::Close(None)),
    };
    ready(Ok(item))
  });
  // handler service for shutdown notification that stop heartbeat task
  let on_shutdown = fn_shutdown(async move || {
    let _ = tx.send(());
  });
  // pipe our service with on_shutdown callback
  Ok(chain(service).and_then(on_shutdown))
}

/// Attach to a virtual machine via websocket
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{key}/attach",
  params(
    ("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 101, description = "Websocket connection"),
  ),
))]
pub async fn vm_attach(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  req: web::HttpRequest,
) -> Result<web::HttpResponse, web::Error> {
  let key = utils::key::parse_resource_key(&path.1)
    .map_err(nanocl_error::http::HttpError::bad_request)?
    .to_string();
  web::ws::start(
    req,
    None::<&str>,
    // inject chat server send to a ws_service factory
    map_config(fn_factory_with_config(ws_attach_service), move |cfg| {
      (key.clone(), cfg, state.clone())
    }),
  )
  .await
}
