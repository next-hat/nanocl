use ntex::{web, rt};
use ntex::util::Bytes;
use ntex::channel::mpsc;

use crate::utils;
use nanocl_error::http::HttpError;
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
      StateData::Secret(data) => {
        if let Err(err) = utils::state::apply_secret(&data, &state, sx).await {
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
      StateData::Secret(data) => {
        if let Err(err) = utils::state::remove_secret(&data, &state, sx).await {
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
  use ntex::http;
  use futures::{TryStreamExt, StreamExt};

  use crate::utils::tests::*;

  async fn apply_state(client: &TestClient, path: &str) {
    let data = parse_statefile(path).unwrap();
    let res = client
      .send_put("/state/apply", Some(&data), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "state apply");
    let mut stream = res.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }
  }

  async fn revert_state(client: &TestClient, path: &str) {
    let data = parse_statefile(path).unwrap();
    let res = client
      .send_put("/state/remove", Some(&data), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "state revert");
    let mut stream = res.into_stream();
    while let Some(item) = stream.next().await {
      item.expect("Correct response");
    }
  }

  #[ntex::test]
  async fn basic() {
    // Generate server
    let client = gen_default_test_client().await;
    // Apply examples/cargo_example.yml
    apply_state(&client, "../../examples/cargo_example.yml").await;
    // ReApply examples/cargo_example.yml
    apply_state(&client, "../../examples/cargo_example.yml").await;
    // Revert examples/cargo_example.yml
    revert_state(&client, "../../examples/cargo_example.yml").await;
    // Apply examples/deploy_example.yml
    apply_state(&client, "../../examples/deploy_example.yml").await;
    // Apply examples/resource_ssl_example.yml
    apply_state(&client, "../../examples/resource_ssl_example.yml").await;
    // ReApply examples/resource_ssl_example.yml
    apply_state(&client, "../../examples/resource_ssl_example.yml").await;
    // Revert examples/resource_ssl_example.yml
    revert_state(&client, "../../examples/resource_ssl_example.yml").await;
    // Revert examples/deploy_example.yml
    revert_state(&client, "../../examples/deploy_example.yml").await;
  }
}
