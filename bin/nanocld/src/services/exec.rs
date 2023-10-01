/*
* Endpoints to manipulate cargoes
*/

use ntex::web;

use bollard_next::exec::{CreateExecOptions, StartExecOptions};

use nanocl_stubs::generic::GenericNspQuery;

use nanocl_utils::http_error::HttpError;

use crate::utils;
use crate::models::DaemonState;

/// Inspect a command executed in a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Exec",
  path = "/exec/{Id}/cargo/inspect",
  params(
    ("Id" = String, Path, description = "Exec id to inspect"),
  ),
  responses(
    (status = 200, description = "Inspect exec infos", body = ExecInspectResponse),
    (status = 404, description = "Exec instance does not exist"),
  ),
))]
#[web::get("/exec/{Id}/cargo/inspect")]
async fn inspect_exec_command(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let infos = utils::exec::inspect_exec_command(&path.1, &state).await?;

  Ok(web::HttpResponse::Ok().json(&infos))
}

// Run an exec command
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Exec",
  request_body = StartExecOptions,
  path = "/exec/{Id}/cargo/start",
  params(
    ("Id" = String, Path, description = "Exec command id"),
  ),
  responses(
    (status = 200, description = "Event Stream of the command output", content_type = "text/event-stream"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/exec/{Id}/cargo/start")]
async fn start_exec_command(
  web::types::Json(payload): web::types::Json<StartExecOptions>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  utils::exec::start_exec_command(&path.1, &payload, &state).await
}

// Create an exec command in a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  request_body = CreateExecOptions,
  path = "/cargoes/{CargoName}/exec",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Event Stream of the command output", content_type = "text/event-stream"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{CargoName}/exec")]
async fn create_exec_command(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CreateExecOptions>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let result = utils::exec::create_exec_command(&key, &payload, &state).await?;

  Ok(web::HttpResponse::Ok().json(&result))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_exec_command);
  config.service(start_exec_command);
  config.service(inspect_exec_command);
}

#[cfg(test)]
mod tests {
  use crate::services::ntex_config;
  use crate::utils::tests::*;

  use bollard_next::exec::{CreateExecOptions, CreateExecResults, StartExecOptions};
  use bollard_next::service::ExecInspectResponse;
  use ntex::http;
  use futures::{TryStreamExt, StreamExt};

  use nanocl_stubs::generic::GenericNspQuery;

  #[ntex::test]
  async fn exec() -> TestRet {
    let srv = gen_server(ntex_config).await;

    const CARGO_NAME: &str = "nstore";

    let mut create_result = srv
      .post(format!("/v0.10/cargoes/{CARGO_NAME}/exec"))
      .query(&GenericNspQuery {
        namespace: Some("system".into()),
      })
      .unwrap()
      .send_json(&CreateExecOptions {
        cmd: Some(vec!["ls".into(), "/".into(), "-lra".into()]),
        attach_stderr: Some(true),
        attach_stdout: Some(true),
        ..Default::default()
      })
      .await?;

    assert_eq!(create_result.status(), http::StatusCode::OK);

    let create_json = create_result.json::<serde_json::Value>().await?;
    println!("json: {:?}", create_json);
    let create_response: CreateExecResults =
      serde_json::from_value(create_json).unwrap();
    let exec_id = create_response.id;

    let start_result = srv
      .post(format!("/v0.10/exec/{exec_id}/cargo/start"))
      .query(&())
      .unwrap()
      .send_json(&StartExecOptions::default())
      .await?;
    assert_eq!(start_result.status(), http::StatusCode::OK);
    let mut stream = start_result.into_stream();

    while let Some(data) = stream.next().await {
      println!("{:?}", data);
    }

    let mut inspect_result = srv
      .get(format!("/v0.10/exec/{exec_id}/cargo/inspect"))
      .query(&())?
      .send()
      .await?;

    assert_eq!(inspect_result.status(), http::StatusCode::OK);
    let inspect_json = inspect_result.json::<serde_json::Value>().await?;
    println!("json: {:?}", inspect_json);
    let inspect_response: ExecInspectResponse =
      serde_json::from_value(inspect_json).unwrap();
    assert_eq!(inspect_response.exit_code, Some(0));

    Ok(())
  }
}
