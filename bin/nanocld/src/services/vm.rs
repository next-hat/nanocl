use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm_config::VmConfigPartial;
use ntex::web;

use bollard_next::Docker;

use crate::utils;
use crate::error::HttpResponseError;
use crate::models::Pool;

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

  utils::vm::create(
    payload,
    &namespace,
    version.to_string(),
    &daemon_conf,
    &docker_api,
    &pool,
  )
  .await?;

  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_vm);
}
