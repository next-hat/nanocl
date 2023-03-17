use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use std::time::Instant;

use ntex::rt;
use ntex::ws;
use ntex::web;
use ntex::util::Bytes;
use ntex::channel::mpsc;
use ntex::channel::oneshot;
use ntex::http::StatusCode;
use ntex::web::{HttpRequest, Error};
use ntex::{pipeline, fn_service, Service};
use ntex::service::{fn_shutdown, map_config, fn_factory_with_config};
use futures::StreamExt;
use futures::future::ready;
use bollard_next::container::AttachContainerOptions;

use nanocl_stubs::cargo::OutputLog;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};

use tokio::io::AsyncWriteExt;

use crate::models::WsConState;
use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::DaemonState;

#[web::get("/vms")]
async fn list_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);

  let vms = utils::vm::list(&namespace, &state.docker_api, &state.pool).await?;

  Ok(web::HttpResponse::Ok().json(&vms))
}

#[web::get("/vms/{name}/inspect")]
async fn inspect_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  let vm = utils::vm::inspect(&key, &state.docker_api, &state.pool).await?;

  Ok(web::HttpResponse::Ok().json(&vm))
}

#[web::post("/vms/{name}/start")]
async fn start_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  repositories::vm::find_by_key(&key, &state.pool).await?;
  utils::vm::start(&key, &state.docker_api).await?;

  Ok(web::HttpResponse::Ok().finish())
}

#[web::post("/vms/{name}/stop")]
async fn stop_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  repositories::vm::find_by_key(&key, &state.pool).await?;
  utils::vm::stop_by_key(&key, &state.docker_api, &state.pool).await?;

  Ok(web::HttpResponse::Ok().finish())
}

#[web::delete("/vms/{name}")]
async fn delete_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  utils::vm::delete(&key, true, &state.docker_api, &state.pool).await?;

  Ok(web::HttpResponse::Ok().finish())
}

#[web::post("/vms")]
async fn create_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<VmConfigPartial>,
  version: web::types::Path<String>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);

  let item = utils::vm::create(
    payload,
    &namespace,
    version.to_string(),
    &state.config,
    &state.docker_api,
    &state.pool,
  )
  .await?;

  Ok(web::HttpResponse::Ok().json(&item))
}

#[web::get("/vms/{name}/histories")]
async fn list_vm_history(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let histories = repositories::vm_config::list_by_vm(key, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&histories))
}

#[web::patch("/vms/{name}")]
async fn patch_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<VmConfigUpdate>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let version = path.0.clone();

  let vm = utils::vm::patch(
    &key,
    &payload,
    &version,
    &state.config,
    &state.docker_api,
    &state.pool,
  )
  .await?;

  Ok(web::HttpResponse::Ok().json(&vm))
}

async fn ws_attach_service(
  (key, sink, state): (String, ws::WsSink, web::types::State<DaemonState>),
) -> Result<
  impl Service<ws::Frame, Response = Option<ws::Message>, Error = io::Error>,
  web::Error,
> {
  // start heartbeat task
  let (tx, rx) = oneshot::channel();
  let con_state = Rc::new(RefCell::new(WsConState::new()));
  rt::spawn(utils::ws::heartbeat(con_state.clone(), sink.clone(), rx));

  let (scmd, mut rcmd) = mpsc::channel::<Result<Bytes, web::Error>>();

  println!("Websocket connection established: {}", key);

  let stream = state
    .docker_api
    .attach_container(
      &format!("{key}.v"),
      Some(AttachContainerOptions::<String> {
        stdin: Some(true),
        stdout: Some(true),
        stderr: Some(true),
        stream: Some(true),
        logs: Some(false),
        detach_keys: Some("ctrl-c".to_string()),
      }),
    )
    .await
    .map_err(|err| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: err.to_string(),
    })?;

  rt::spawn(async move {
    let mut output = stream.output;
    while let Some(output) = output.next().await {
      let output = match output {
        Ok(output) => output,
        Err(e) => {
          println!("Error reading from container: {}", e);
          break;
        }
      };

      let outputlog: OutputLog = output.into();

      println!("{outputlog:#?}");

      let output = serde_json::to_vec(&outputlog);

      let mut output = match output {
        Ok(output) => output,
        Err(e) => {
          println!("Error serializing output: {}", e);
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
          println!("Error reading from container: {}", e);
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
  Ok(pipeline(service).and_then(on_shutdown))
}

/// Entry point for our route
async fn vm_attach(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  req: HttpRequest,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, Error> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);

  web::ws::start(
    req,
    // inject chat server send to a ws_service factory
    map_config(fn_factory_with_config(ws_attach_service), move |cfg| {
      (key.clone(), cfg, state.clone())
    }),
  )
  .await
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_vm);
  config.service(create_vm);
  config.service(delete_vm);
  config.service(inspect_vm);
  config.service(start_vm);
  config.service(stop_vm);
  config.service(list_vm_history);
  config.service(patch_vm);
  config.service(
    web::resource("/vms/{name}/attach").route(web::get().to(vm_attach)),
  );
}

#[cfg(test)]
mod tests {
  use crate::services::ntex_config;

  use ntex::http::StatusCode;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn list_vm() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let resp = srv.get("/v0.2/vms").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    Ok(())
  }
}
