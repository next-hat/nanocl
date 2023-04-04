use ntex::web;

use crate::utils;
use crate::error::HttpError;
use crate::models::{StateData, DaemonState};

#[web::put("/state/apply")]
pub(crate) async fn apply(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  version: web::types::Path<String>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::apply_deployment(&data, &version, &state).await?;
    }
    StateData::Cargo(data) => {
      utils::state::apply_cargo(&data, &version, &state).await?;
    }
    StateData::Resource(data) => {
      utils::state::apply_resource(&data, &state).await?;
    }
  }
  Ok(web::HttpResponse::Ok().finish())
}

#[web::put("/state/revert")]
pub(crate) async fn revert(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  match utils::state::parse_state(&payload)? {
    StateData::Deployment(data) => {
      utils::state::revert_deployment(&data, &state).await?;
    }
    StateData::Cargo(data) => {
      utils::state::revert_cargo(&data, &state).await?;
    }
    StateData::Resource(data) => {
      utils::state::revert_resource(&data, &state).await?;
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
  pub(crate) async fn basic() -> TestRet {
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

    let data = parse_state_file("../../examples/deploy_example.yml")?;
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

    let data = parse_state_file("../../examples/deploy_example.yml")?;
    let req = srv
      .put("/v0.2/state/revert")
      .send_json(&data)
      .await
      .unwrap();
    assert_eq!(req.status(), 200);

    Ok(())
  }
}
