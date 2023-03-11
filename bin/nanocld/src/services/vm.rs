use ntex::web;
use ntex::util::BytesMut;
use futures::StreamExt;
use bollard_next::container::{StartContainerOptions, AttachContainerOptions};

use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm_config::VmConfigPartial;

use bollard_next::Docker;
use tokio::io::AsyncWriteExt;

use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::Pool;

#[web::get("/vms")]
async fn list_vm(
  docker_api: web::types::State<Docker>,
  pool: web::types::State<Pool>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);

  let vms = utils::vm::list(&namespace, &docker_api, &pool).await?;

  Ok(web::HttpResponse::Ok().json(&vms))
}

#[web::get("/vms/{name}/inspect")]
async fn inspect_vm(
  docker_api: web::types::State<Docker>,
  pool: web::types::State<Pool>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  let vm = utils::vm::inspect(&key, &docker_api, &pool).await?;

  Ok(web::HttpResponse::Ok().json(&vm))
}

#[web::post("/vms/{name}/start")]
async fn start_vm(
  docker_api: web::types::State<Docker>,
  pool: web::types::State<Pool>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  repositories::vm::find_by_key(&key, &pool).await?;
  utils::vm::start(&key, &docker_api).await?;

  Ok(web::HttpResponse::Ok().finish())
}

#[web::post("/vms/{name}/stop")]
async fn stop_vm(
  docker_api: web::types::State<Docker>,
  pool: web::types::State<Pool>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  repositories::vm::find_by_key(&key, &pool).await?;
  utils::vm::stop(&key, &docker_api).await?;

  Ok(web::HttpResponse::Ok().finish())
}

#[web::delete("/vms/{name}")]
async fn delete_vm(
  docker_api: web::types::State<Docker>,
  pool: web::types::State<Pool>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);

  utils::vm::delete(&key, true, &docker_api, &pool).await?;

  Ok(web::HttpResponse::Ok().finish())
}

#[web::post("/vms")]
async fn create_vm(
  docker_api: web::types::State<Docker>,
  pool: web::types::State<Pool>,
  daemon_conf: web::types::State<DaemonConfig>,
  web::types::Json(payload): web::types::Json<VmConfigPartial>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  version: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);

  let item = utils::vm::create(
    payload,
    &namespace,
    version.to_string(),
    &daemon_conf,
    &docker_api,
    &pool,
  )
  .await?;

  Ok(web::HttpResponse::Ok().json(&item))
}

#[web::post("/vms/{name}/attach")]
pub async fn attach_vm(
  path: web::types::Path<(String, String)>,
  docker_api: web::types::State<Docker>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  mut body: web::types::Payload,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let vm_key = utils::key::gen_key(&namespace, &path.1);
  let vm_container = format!("{vm_key}.vm");

  // req.io()
  // Attach to the container's input stream
  let attach_options = AttachContainerOptions::<String> {
    stream: Some(true),
    stdin: Some(true),
    stdout: Some(true),
    stderr: Some(true),
    logs: Some(true),
    ..Default::default()
  };
  let mut attach_stream = docker_api
    .attach_container(&vm_container, Some(attach_options))
    .await?;

  // Start the container if it's not already running
  let start_options = StartContainerOptions { detach_keys: "" };
  docker_api
    .start_container(&vm_container, Some(start_options))
    .await?;

  while let Some(chunk) = body.next().await {
    let data = chunk.unwrap().to_vec();
    let _ = attach_stream.input.write_all(&data).await;
  }

  // Return a response with the container's output and error streams
  let output = attach_stream.output.map(|chunk| {
    let chunk = chunk.unwrap();
    let buf = BytesMut::from_iter(chunk.into_bytes().to_vec());
    Ok::<_, std::io::Error>(buf.freeze())
  });

  let response = web::HttpResponse::Ok()
    .content_type("nanocl-streaming/v1")
    .keep_alive()
    .upgrade("tcp")
    .set_header("connection", "upgrade")
    .streaming(output);

  Ok(response)
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_vm);
  config.service(create_vm);
  config.service(delete_vm);
  config.service(inspect_vm);
  config.service(start_vm);
  config.service(stop_vm);
  config.service(attach_vm);
}
