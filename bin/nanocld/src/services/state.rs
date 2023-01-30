use ntex::web;
use ntex::http::StatusCode;

use crate::utils;
use crate::models::{Pool, StateData};

use crate::error::HttpResponseError;

#[web::put("/state/apply")]
async fn apply(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  docker_api: web::types::State<bollard::Docker>,
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::apply_deployment(data, &docker_api, &pool).await?;
    }
    StateData::Cargo(data) => {
      utils::state::apply_cargo(data, &docker_api, &pool).await?;
    }
    StateData::Resource(data) => {
      utils::state::apply_resource(data, &pool).await?;
    }
  }
  Ok(web::HttpResponse::Ok().finish())
}

#[web::put("/state/revert")]
async fn revert(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  docker_api: web::types::State<bollard::Docker>,
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::revert_deployment(data, &docker_api, &pool).await?;
    }
    StateData::Cargo(data) => {
      utils::state::revert_cargo(data, &docker_api, &pool).await?;
    }
    StateData::Resource(data) => {
      utils::state::revert_resource(data, &pool).await?;
    }
  }
  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(cfg: &mut web::ServiceConfig) {
  cfg.service(apply);
  cfg.service(revert);
}
