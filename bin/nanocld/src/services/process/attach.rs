use std::{cell::RefCell, io, rc::Rc, time::Instant};

use futures::{StreamExt, future::ready};
use ntex::{
  Service,
  channel::{mpsc, oneshot},
  fn_service, rt,
  util::Bytes,
  web, ws,
};
use tokio::io::AsyncWriteExt;

use bollard_next::{
  container::AttachContainerResults, query_parameters::AttachContainerOptions,
};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::OutputLog;

use crate::{
  models::{SystemState, WsConState},
  utils,
};

fn ws_attach_service(
  stream: AttachContainerResults,
) -> impl Service<ws::WsSink, ws::Frame, Res = Option<ws::Message>, Error = io::Error>
{
  let con_state = Rc::new(RefCell::new(WsConState::new()));
  let heartbeat_state = con_state.clone();
  let (tx, rx) = oneshot::channel();
  let (s_cmd, r_cmd) = mpsc::channel::<Result<Bytes, io::Error>>();
  let startup = Rc::new(RefCell::new(Some((stream, rx, r_cmd))));
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
  ntex::service(service)
    .readiness(async move |sink: &ws::WsSink| {
      let startup = startup.borrow_mut().take();
      if let Some((stream, rx, mut r_cmd)) = startup {
        rt::spawn(utils::ws::heartbeat(
          heartbeat_state.clone(),
          sink.clone(),
          rx,
        ));
        let sink = sink.clone();
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
      }
      Ok(())
    })
    .shutdown(async move |_| {
      let _ = tx.send(());
    })
}

/// Attach to a process via websocket
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{name}/attach",
  params(
    ("name" = String, Path, description = "Name or id of the container"),
  ),
  responses(
    (status = 101, description = "Websocket connection"),
  ),
))]
pub async fn attach_process(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  req: web::HttpRequest,
) -> HttpResult<web::HttpResponse> {
  if let Err(error) = ws::verify_handshake(req.head()) {
    return utils::ws::response(Err(error.into()));
  }
  let (_, name) = path.into_inner();
  // Docker must accept the attachment before we send the WebSocket upgrade.
  let stream = state
    .inner
    .docker_api
    .attach_container(
      &name,
      Some(AttachContainerOptions {
        stdin: true,
        stdout: true,
        stderr: true,
        stream: true,
        logs: false,
        detach_keys: Some("ctrl-c".to_owned()),
      }),
    )
    .await
    .map_err(HttpError::internal_server_error)?;
  utils::ws::response(
    web::ws::start(&req, None, ws_attach_service(stream)).await,
  )
}

#[cfg(test)]
mod tests {
  use ntex::ws::error::WsClientError;
  use ntex::{SharedCfg, http::StatusCode, ws::WsClient};

  use crate::utils::tests::gen_default_test_system;

  #[ntex::test]
  async fn missing_process_rejects_websocket_upgrade() {
    let system = gen_default_test_system().await;
    let path = format!(
      "/processes/nanocl-attach-missing-{}/attach",
      uuid::Uuid::new_v4()
    );

    // Protocol validation must still happen before Docker attachment.
    let response = system.client.send_get(&path, None::<String>).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Use a real upgrade request: a test-only handler call cannot prove that
    // the HTTP error is sent before ntex takes ownership of the connection.
    let request = system.client.get(&path);
    let error =
      WsClient::new(request.get_uri().to_string(), SharedCfg::default())
        .connect()
        .await
        .err()
        .expect("a missing process must reject the WebSocket upgrade");
    assert!(
      matches!(
        *error,
        WsClientError::InvalidResponseStatus(StatusCode::INTERNAL_SERVER_ERROR)
      ),
      "{error}"
    );
  }
}
