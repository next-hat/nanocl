use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use std::time::Instant;

use ntex::rt;
use ntex::ws;
use ntex::web;
use ntex::http;
use ntex::util::Bytes;
use ntex::channel::mpsc;
use ntex::channel::oneshot;
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

use crate::{utils, repositories};
use nanocl_utils::http_error::HttpError;
use crate::models::{DaemonState, WsConState};

/// List virtual machines
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms",
  params(
    ("Namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "List of virtual machine", body = [VmSummary]),
  ),
))]
#[web::get("/vms")]
pub(crate) async fn list_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);

  let vms =
    utils::vm::list_by_namespace(&namespace, &state.docker_api, &state.pool)
      .await?;

  Ok(web::HttpResponse::Ok().json(&vms))
}

/// Inspect a virtual machine
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{Name}/inspect",
  params(
    ("Name" = String, Path, description = "The name of the virtual machine"),
    ("Namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "Detailed information about a virtual machine", body = VmInspect),
  ),
))]
#[web::get("/vms/{name}/inspect")]
pub(crate) async fn inspect_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  let vm =
    utils::vm::inspect_by_key(&key, &state.docker_api, &state.pool).await?;

  Ok(web::HttpResponse::Ok().json(&vm))
}

/// Start a virtual machine
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Vms",
  path = "/vms/{Name}/start",
  params(
    ("Name" = String, Path, description = "The name of the virtual machine"),
    ("Namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "The virtual machine has been started"),
  ),
))]
#[web::post("/vms/{name}/start")]
pub(crate) async fn start_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  repositories::vm::find_by_key(&key, &state.pool).await?;
  utils::vm::start_by_key(&key, &state.docker_api).await?;

  Ok(web::HttpResponse::Ok().finish())
}

/// Stop a virtual machine
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Vms",
  path = "/vms/{Name}/stop",
  params(
    ("Name" = String, Path, description = "The name of the virtual machine"),
    ("Namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "The virtual machine has been stopped"),
  ),
))]
#[web::post("/vms/{name}/stop")]
pub(crate) async fn stop_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  repositories::vm::find_by_key(&key, &state.pool).await?;
  utils::vm::stop_by_key(&key, &state.docker_api, &state.pool).await?;

  Ok(web::HttpResponse::Ok().finish())
}

/// Delete a virtual machine
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Vms",
  path = "/vms/{Name}",
  params(
    ("Name" = String, Path, description = "The name of the virtual machine"),
    ("Namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "The virtual machine has been deleted"),
  ),
))]
#[web::delete("/vms/{name}")]
pub(crate) async fn delete_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  utils::vm::delete_by_key(&key, true, &state.docker_api, &state.pool).await?;

  Ok(web::HttpResponse::Ok().finish())
}

/// Create a virtual machine
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Vms",
  path = "/vms",
  request_body = VmConfigPartial,
  params(
    ("Namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "The virtual machine has been created", body = Vm),
  ),
))]
#[web::post("/vms")]
pub(crate) async fn create_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<VmConfigPartial>,
  version: web::types::Path<String>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);

  let item = utils::vm::create(&payload, &namespace, &version, &state).await?;

  Ok(web::HttpResponse::Ok().json(&item))
}

/// List virtual machine histories
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{Name}/histories",
  params(
    ("Name" = String, Path, description = "The name of the virtual machine"),
    ("Namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "The virtual machine histories have been listed", body = [VmConfig]),
  ),
))]
#[web::get("/vms/{name}/histories")]
pub(crate) async fn list_vm_history(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let histories =
    repositories::vm_config::list_by_vm_key(&key, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&histories))
}

/// Patch a virtual machine config meaning merging current config with the new one and add history entry
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Vms",
  request_body = VmConfigUpdate,
  path = "/vms/{Name}",
  params(
    ("Name" = String, Path, description = "Name of the virtual machine"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "Updated virtual machine", body = Vm),
    (status = 404, description = "Virtual machine not found", body = ApiError),
  ),
))]
#[web::patch("/vms/{name}")]
pub(crate) async fn patch_vm(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<VmConfigUpdate>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let version = path.0.clone();

  let vm = utils::vm::patch(&key, &payload, &version, &state).await?;

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
    .map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: err.to_string(),
    })?;

  rt::spawn(async move {
    let mut output = stream.output;
    while let Some(output) = output.next().await {
      let output = match output {
        Ok(output) => output,
        Err(e) => {
          log::error!("Error reading from container: {}", e);
          break;
        }
      };

      let outputlog: OutputLog = output.into();
      let output = serde_json::to_vec(&outputlog);

      let mut output = match output {
        Ok(output) => output,
        Err(e) => {
          log::error!("Error serializing output: {}", e);
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
          log::error!("Error reading from container: {}", e);
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

/// Attach to a virtual machine via websocket
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{Name}/attach",
  params(
    ("Name" = String, Path, description = "Name of the virtual machine"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the virtual machine"),
  ),
  responses(
    (status = 101, description = "Websocket connection"),
  ),
))]
pub(crate) async fn vm_attach(
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

  use ntex::http;

  use crate::utils::tests::*;

  #[ntex::test]
  pub(crate) async fn list_vm() -> TestRet {
    let srv = gen_server(ntex_config).await;
    let resp = srv.get("/v0.2/vms").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      http::StatusCode::OK,
      "Expect status to be {} got {}",
      http::StatusCode::OK,
      status
    );
    Ok(())
  }
}
