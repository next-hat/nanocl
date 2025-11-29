use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::node::{DeleteNodeCargoParams, StartNodeCargoParams};

use crate::{
  models::{CargoDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Start cargo instances on the node used for node to node communication
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Nodes",
  path = "/nodes/cargoes/start",
  request_body = StartNodeCargoParams,
  responses(
    (status = 200, description = "Cargoes sucessfully started"),
  ),
))]
#[web::get("/nodes/cargoes/start")]
pub async fn start_node_cargo(
  state: web::types::State<SystemState>,
  params: web::types::Json<StartNodeCargoParams>,
) -> HttpResult<web::HttpResponse> {
  let cargo =
    CargoDb::transform_read_by_pk(&params.cargo_key, &state.inner.pool).await?;
  utils::container::cargo::start(&cargo, params.replicas, &state).await?;
  Ok(web::HttpResponse::Ok().finish())
}

/// Delete cargo instances on the node used for node to node communication
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Nodes",
  path = "/nodes/cargoes",
  request_body = DeleteNodeCargoParams,
  responses(
    (status = 200, description = "Cargoes sucessfully deleted"),
  ),
))]
#[web::delete("/nodes/cargoes")]
pub async fn delete_node_cargo(
  state: web::types::State<SystemState>,
  params: web::types::Json<DeleteNodeCargoParams>,
) -> HttpResult<web::HttpResponse> {
  utils::container::cargo::delete_instances(&params.cargo_key, false, &state)
    .await?;
  Ok(web::HttpResponse::Ok().finish())
}
