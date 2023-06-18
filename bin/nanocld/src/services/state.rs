use ntex::{web, rt};
use ntex::util::Bytes;
use ntex::channel::mpsc;

use crate::utils;
use nanocl_utils::http_error::HttpError;
use crate::models::{StateData, DaemonState};

#[web::put("/state/apply")]
pub(crate) async fn apply(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  version: web::types::Path<String>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let state_file = utils::state::parse_state(&payload)?;
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  rt::spawn(async move {
    match state_file {
      StateData::Deployment(data) => {
        if let Err(err) =
          utils::state::apply_deployment(&data, &version, &state, sx).await
        {
          log::warn!("{err}");
        }
      }
      StateData::Cargo(data) => {
        if let Err(err) =
          utils::state::apply_cargo(&data, &version, &state, sx).await
        {
          log::warn!("{err}");
        }
      }
      StateData::VirtualMachine(data) => {
        if let Err(err) =
          utils::state::apply_vm(&data, &version, &state, sx).await
        {
          log::warn!("{err}");
        }
      }
      StateData::Resource(data) => {
        if let Err(err) = utils::state::apply_resource(&data, &state, sx).await
        {
          log::warn!("{err}");
        }
      }
    };
  });

  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(rx),
  )
}

#[web::put("/state/remove")]
pub(crate) async fn remove(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let state_file = utils::state::parse_state(&payload)?;
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();

  rt::spawn(async move {
    match state_file {
      StateData::Deployment(data) => {
        if let Err(err) =
          utils::state::remove_deployment(&data, &state, sx).await
        {
          log::warn!("{err}");
        }
      }
      StateData::Cargo(data) => {
        if let Err(err) = utils::state::remove_cargo(&data, &state, sx).await {
          log::warn!("{err}");
        }
      }
      StateData::VirtualMachine(data) => {
        if let Err(err) = utils::state::remove_vm(&data, &state, sx).await {
          log::warn!("{err}");
        }
      }
      StateData::Resource(data) => {
        if let Err(err) = utils::state::remove_resource(&data, &state, sx).await
        {
          log::warn!("{err}");
        }
      }
    };
  });

  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(rx),
  )
}

pub fn ntex_config(cfg: &mut web::ServiceConfig) {
  cfg.service(apply);
  cfg.service(remove);
}

#[cfg(test)]
mod tests {
  use futures::{TryStreamExt, StreamExt};

  use crate::services::ntex_config;

  use crate::utils::tests::*;

  #[ntex::test]
  pub(crate) async fn basic() -> TestRet {
    // Generate server
    let srv = generate_server(ntex_config).await;

    // Apply examples/cargo_example.yml
    let data = parse_state_file("../../examples/cargo_example.yml")?;
    let req = srv.put("/v0.5/state/apply").send_json(&data).await.unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    // Apply examples/deploy_example.yml
    let data = parse_state_file("../../examples/cargo_example.yml")?;
    let req = srv.put("/v0.5/state/apply").send_json(&data).await.unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    // Revert examples/cargo_example.yml
    let data = parse_state_file("../../examples/cargo_example.yml")?;
    let req = srv
      .put("/v0.5/state/remove")
      .send_json(&data)
      .await
      .unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    // Apply examples/deploy_example.yml
    let data = parse_state_file("../../examples/deploy_example.yml")?;
    let req = srv.put("/v0.5/state/apply").send_json(&data).await.unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    // Apply examples/resource_ssl_example.yml
    let data = parse_state_file("../../examples/resource_ssl_example.yml")?;
    let req = srv.put("/v0.5/state/apply").send_json(&data).await.unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    // Apply examples/resource_ssl_example.yml
    let data = parse_state_file("../../examples/resource_ssl_example.yml")?;
    let req = srv.put("/v0.5/state/apply").send_json(&data).await.unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    // Revert examples/resource_ssl_example.yml
    let data = parse_state_file("../../examples/resource_ssl_example.yml")?;
    let req = srv
      .put("/v0.5/state/remove")
      .send_json(&data)
      .await
      .unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    // Revert examples/deploy_example.yml
    let data = parse_state_file("../../examples/deploy_example.yml")?;
    let req = srv
      .put("/v0.5/state/remove")
      .send_json(&data)
      .await
      .unwrap();
    assert_eq!(req.status(), 200);
    let mut stream = req.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }

    Ok(())
  }
}
