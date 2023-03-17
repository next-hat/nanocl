use ntex::web;

use crate::utils;
use crate::models::{StateData, DaemonState};

use crate::error::HttpResponseError;

#[web::put("/state/apply")]
async fn apply(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  version: web::types::Path<String>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::apply_deployment(
        data,
        version.into_inner(),
        &state.docker_api,
        &state.pool,
        &state.event_emitter,
      )
      .await?;
    }
    StateData::Cargo(data) => {
      utils::state::apply_cargo(
        data,
        version.into_inner(),
        &state.docker_api,
        &state.pool,
        &state.event_emitter,
      )
      .await?;
    }
    StateData::Resource(data) => {
      utils::state::apply_resource(data, &state.pool, &state.event_emitter)
        .await?;
    }
  }
  Ok(web::HttpResponse::Ok().finish())
}

#[web::put("/state/revert")]
async fn revert(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::revert_deployment(
        data,
        &state.docker_api,
        &state.pool,
        &state.event_emitter,
      )
      .await?;
    }
    StateData::Cargo(data) => {
      utils::state::revert_cargo(
        data,
        &state.docker_api,
        &state.pool,
        &state.event_emitter,
      )
      .await?;
    }
    StateData::Resource(data) => {
      utils::state::revert_resource(data, &state.pool, &state.event_emitter)
        .await?;
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
  use crate::services::ntex_config;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;

    let data = parse_state_file("../../examples/cargo_example.yml")?;

    let req = srv.put("/v0.2/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/cargo_example.yml")?;

    let req = srv.put("/v0.2/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/cargo_example.yml")?;

    let req = srv
      .put("/v0.2/state/revert")
      .send_json(&data)
      .await
      .unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_custom.yml")?;
    let req = srv.put("/v0.2/state/apply").send_json(&data).await.unwrap();
    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_ssl_example.yml")?;

    let req = srv.put("/v0.2/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_ssl_example.yml")?;

    let req = srv.put("/v0.2/state/apply").send_json(&data).await.unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_ssl_example.yml")?;

    let req = srv
      .put("/v0.2/state/revert")
      .send_json(&data)
      .await
      .unwrap();

    assert_eq!(req.status(), 200);

    let data = parse_state_file("../../examples/resource_custom.yml")?;
    let req = srv
      .put("/v0.2/state/revert")
      .send_json(&data)
      .await
      .unwrap();
    assert_eq!(req.status(), 200);

    Ok(())
  }
}
