use ntex::web;
use ntex::http::StatusCode;

use crate::utils;
use crate::models::Pool;

use nanocl_models::state::{
  StateConfig, StateDeployment, StateCargo, StateResources,
};

use crate::error::HttpResponseError;

#[web::put("/state/apply")]
async fn apply(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  docker_api: web::types::State<bollard::Docker>,
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let meta = serde_json::from_value::<StateConfig>(payload.to_owned())
    .map_err(|err| HttpResponseError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("unable to serialize payload {err}"),
    })?;

  match meta.r#type.as_str() {
    "Deployment" => {
      println!("Deployment");
      let data =
        serde_json::from_value::<StateDeployment>(payload).map_err(|err| {
          HttpResponseError {
            status: StatusCode::BAD_REQUEST,
            msg: format!(
              "unable to serialize payload for type {} error: {err}",
              meta.r#type
            ),
          }
        })?;
      utils::state::deployment(data, &docker_api, &pool).await?;
    }
    "Cargo" => {
      let data =
        serde_json::from_value::<StateCargo>(payload).map_err(|err| {
          HttpResponseError {
            status: StatusCode::BAD_REQUEST,
            msg: format!(
              "unable to serialize payload for type {} error: {err}",
              meta.r#type
            ),
          }
        })?;
      utils::state::cargo(data, &docker_api, &pool).await?;
    }
    "Resource" => {
      let data =
        serde_json::from_value::<StateResources>(payload).map_err(|err| {
          HttpResponseError {
            status: StatusCode::BAD_REQUEST,
            msg: format!(
              "unable to serialize payload for type {} error: {err}",
              meta.r#type
            ),
          }
        })?;
      utils::state::resource(data, &pool).await?;
    }
    _ => {
      return Err(HttpResponseError {
        status: StatusCode::BAD_REQUEST,
        msg: format!("unknown type {}", meta.r#type),
      });
    }
  }
  Ok(web::HttpResponse::Ok().finish())
}

#[web::put("/state/revert")]
async fn revert() -> Result<web::HttpResponse, HttpResponseError> {
  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(cfg: &mut web::ServiceConfig) {
  cfg.service(apply);
  cfg.service(revert);
}
