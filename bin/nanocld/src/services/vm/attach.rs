use std::{cell::RefCell, io, rc::Rc, time::Instant};

use bollard_next::container::AttachContainerOptions;
use futures::{future::ready, StreamExt};
use nanocl_error::http::HttpError;
use nanocl_stubs::{generic::GenericNspQuery, process::OutputLog};
use ntex::{
  chain,
  channel::{mpsc, oneshot},
  fn_service, rt,
  service::{fn_factory_with_config, fn_shutdown, map_config},
  util::Bytes,
  web, ws, Service,
};
use tokio::io::AsyncWriteExt;

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
  let (scmd, mut rcmd) = mpsc::channel::<Result<Bytes, web::Error>>();
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
      let outputlog: OutputLog = output.into();
      let output = serde_json::to_vec(&outputlog);
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
    while let Some(cmd) = rcmd.next().await {
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
        let _ = scmd.send(Ok(text));
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
  Ok(chain(service).and_then(on_shutdown))
}

/// Attach to a virtual machine via websocket
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{name}/attach",
  params(
    ("name" = String, Path, description = "Name of the virtual machine"),
    ("namespace" = Option<String>, Query, description = "Namespace of the virtual machine"),
  ),
  responses(
    (status = 101, description = "Websocket connection"),
  ),
))]
pub async fn vm_attach(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  req: web::HttpRequest,
  qs: web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, web::Error> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  web::ws::start(
    req,
    // inject state to ws_attach_service factory
    map_config(fn_factory_with_config(ws_attach_service), move |cfg| {
      (key.clone(), cfg, state.clone())
    }),
  )
  .await
}
