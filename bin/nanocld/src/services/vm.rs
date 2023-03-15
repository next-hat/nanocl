use ntex::web;

use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};

use bollard_next::Docker;

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
  utils::vm::stop_by_key(&key, &docker_api, &pool).await?;

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

#[web::get("/vms/{name}/histories")]
async fn list_vm_history(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let histories = repositories::vm_config::list_by_vm(key, &pool).await?;
  Ok(web::HttpResponse::Ok().json(&histories))
}

#[web::patch("/vms/{name}")]
async fn patch_vm(
  pool: web::types::State<Pool>,
  daemon_conf: web::types::State<DaemonConfig>,
  docker_api: web::types::State<Docker>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<VmConfigUpdate>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let version = path.0.clone();

  let vm = utils::vm::patch(
    &key,
    &payload,
    &version,
    &daemon_conf,
    &docker_api,
    &pool,
  )
  .await?;

  Ok(web::HttpResponse::Ok().json(&vm))
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
