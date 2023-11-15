use ntex::{rt, web};
use ntex::util::Bytes;
use ntex::channel::mpsc;

use nanocl_error::http::HttpError;
use nanocl_stubs::state::StateApplyQuery;

use crate::utils;
use crate::models::{StateData, DaemonState};

#[web::put("/state/apply")]
pub(crate) async fn apply(
  web::types::Json(payload): web::types::Json<serde_json::Value>,
  qs: web::types::Query<StateApplyQuery>,
  version: web::types::Path<String>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let state_file = utils::state::parse_state(&payload)?;
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();
  rt::spawn(async move {
    let res = match state_file {
      StateData::Deployment(data) => {
        utils::state::apply_deployment(&data, &version, &state, &qs, sx).await
      }
      StateData::Cargo(data) => {
        utils::state::apply_cargo(&data, &version, &state, &qs, sx).await
      }
      StateData::VirtualMachine(data) => {
        utils::state::apply_vm(&data, &version, &state, &qs, sx).await
      }
      StateData::Resource(data) => {
        utils::state::apply_resource(&data, &state, &qs, sx).await
      }
      StateData::Secret(data) => {
        utils::state::apply_secret(&data, &state, &qs, sx).await
      }
      StateData::Job(data) => {
        utils::state::apply_job(&data, &state, &qs, sx).await
      }
    };
    if let Err(err) = res {
      log::warn!("{err}");
    }
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
    let res = match state_file {
      StateData::Deployment(data) => {
        utils::state::remove_deployment(&data, &state, sx).await
      }
      StateData::Cargo(data) => {
        utils::state::remove_cargo(&data, &state, sx).await
      }
      StateData::VirtualMachine(data) => {
        utils::state::remove_vm(&data, &state, sx).await
      }
      StateData::Resource(data) => {
        utils::state::remove_resource(&data, &state, sx).await
      }
      StateData::Secret(data) => {
        utils::state::remove_secret(&data, &state, sx).await
      }
      _ => Ok(()), // StateData::Job(data) => utils::state::remove_job(&data, &state, sx).await,
    };
    if let Err(err) = res {
      log::warn!("{err}");
    }
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
  use nanocl_stubs::state::StateApplyQuery;
  use ntex::http;
  use futures::{TryStreamExt, StreamExt};

  use crate::utils::tests::*;

  async fn apply_state(
    client: &TestClient,
    path: &str,
    options: Option<&StateApplyQuery>,
  ) {
    let data = parse_statefile(path).unwrap();
    let res = client.send_put("/state/apply", Some(&data), options).await;
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
    apply_state(&client, "../../examples/cargo_example.yml", None).await;
    // ReApply examples/cargo_example.yml
    apply_state(&client, "../../examples/cargo_example.yml", None).await;
    // ReApply examples/cargo_example.yml with reload
    apply_state(
      &client,
      "../../examples/cargo_example.yml",
      Some(&StateApplyQuery { reload: Some(true) }),
    )
    .await;
    // Revert examples/cargo_example.yml
    revert_state(&client, "../../examples/cargo_example.yml").await;
    // Apply examples/deploy_example.yml
    apply_state(&client, "../../examples/deploy_example.yml", None).await;
    // Apply examples/resource_ssl_example.yml
    apply_state(&client, "../../examples/resource_ssl_example.yml", None).await;
    // ReApply examples/resource_ssl_example.yml
    apply_state(&client, "../../examples/resource_ssl_example.yml", None).await;
    // Revert examples/resource_ssl_example.yml
    revert_state(&client, "../../examples/resource_ssl_example.yml").await;
    // ReApply examples/deploy_secrets.yml
    apply_state(&client, "../../examples/deploy_secrets.yml", None).await;
    // Revert examples/deploy_secrets.yml
    revert_state(&client, "../../examples/deploy_secrets.yml").await;
    // ReApply examples/cargo_autoremove.yml
    apply_state(&client, "../../examples/cargo_autoremove.yml", None).await;
    // Revert examples/cargo_autoremove.yml
    revert_state(&client, "../../examples/cargo_autoremove.yml").await;
    // ReApply examples/success_init_container.yml
    apply_state(&client, "../../examples/success_init_container.yml", None)
      .await;
    // Revert examples/success_init_container.yml
    revert_state(&client, "../../examples/success_init_container.yml").await;
    // ReApply examples/fail_init_container.yml
    apply_state(&client, "../../examples/fail_init_container.yml", None).await;
    // Revert examples/fail_init_container.yml
    revert_state(&client, "../../examples/fail_init_container.yml").await;
    // Revert examples/deploy_example.yml
    revert_state(&client, "../../examples/deploy_example.yml").await;
    // Apply examples/secret_env.yml
    apply_state(&client, "../../examples/secret_env.yml", None).await;
    // ReApply examples/secret_env.yml
    apply_state(&client, "../../examples/secret_env.yml", None).await;
    // ReApply examples/secret_env.yml with reload
    apply_state(
      &client,
      "../../examples/secret_env.yml",
      Some(&StateApplyQuery { reload: Some(true) }),
    )
    .await;
    // Revert examples/secret_env.yml
    revert_state(&client, "../../examples/secret_env.yml").await;
  }
}
