use ntex::web;

use crate::event::EventEmitterPtr;
use crate::utils;
use crate::models::{Pool, StateData};

use crate::error::HttpResponseError;

#[web::put("/state/apply")]
async fn apply(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  docker_api: web::types::State<bollard_next::Docker>,
  pool: web::types::State<Pool>,
  event_emitter: web::types::State<EventEmitterPtr>,
) -> Result<web::HttpResponse, HttpResponseError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::apply_deployment(data, &docker_api, &pool, &event_emitter)
        .await?;
    }
    StateData::Cargo(data) => {
      utils::state::apply_cargo(data, &docker_api, &pool, &event_emitter)
        .await?;
    }
    StateData::Resource(data) => {
      utils::state::apply_resource(data, &pool, &event_emitter).await?;
    }
  }
  Ok(web::HttpResponse::Ok().finish())
}

#[web::put("/state/revert")]
async fn revert(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  docker_api: web::types::State<bollard_next::Docker>,
  pool: web::types::State<Pool>,
  event_emitter: web::types::State<EventEmitterPtr>,
) -> Result<web::HttpResponse, HttpResponseError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::revert_deployment(data, &docker_api, &pool, &event_emitter)
        .await?;
    }
    StateData::Cargo(data) => {
      utils::state::revert_cargo(data, &docker_api, &pool, &event_emitter)
        .await?;
    }
    StateData::Resource(data) => {
      utils::state::revert_resource(data, &pool, &event_emitter).await?;
    }
  }
  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(cfg: &mut web::ServiceConfig) {
  cfg.service(apply);
  cfg.service(revert);
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic_test() -> TestRet {
    let srv = generate_server(ntex_config).await;

    let data = parse_state_file("../../examples/cargo_example.yml")?;

    let req = srv.put("/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/cargo_example.yml")?;

    let req = srv.put("/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/cargo_example.yml")?;

    let req = srv.put("/state/revert").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_example.yml")?;

    let req = srv.put("/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_example.yml")?;

    let req = srv.put("/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_example.yml")?;

    let req = srv.put("/state/revert").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    Ok(())
  }
}
