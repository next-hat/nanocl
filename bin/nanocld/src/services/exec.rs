/*
* Endpoints to manipulate cargoes
*/

use ntex::web;

use nanocl_error::http::HttpResult;

use bollard_next::exec::{CreateExecOptions, StartExecOptions};
use nanocl_stubs::generic::GenericNspQuery;

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
pub(crate) async fn inspect_exec_command(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
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
pub(crate) async fn start_exec_command(
  web::types::Json(payload): web::types::Json<StartExecOptions>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
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
pub(crate) async fn create_exec_command(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CreateExecOptions>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let result = utils::exec::create_exec_command(&key, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().json(&result))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_exec_command);
  config.service(start_exec_command);
  config.service(inspect_exec_command);
}

#[cfg(test)]
mod tests {

  use ntex::http;
  use futures::{TryStreamExt, StreamExt};
  use bollard_next::service::ExecInspectResponse;
  use bollard_next::exec::{CreateExecOptions, CreateExecResults, StartExecOptions};

  use nanocl_stubs::generic::GenericNspQuery;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn exec() {
    const CARGO_NAME: &str = "nstore";
    let client = gen_default_test_client().await;
    let mut res = client
      .send_post(
        &format!("/cargoes/{CARGO_NAME}/exec"),
        Some(&CreateExecOptions {
          cmd: Some(vec!["ls".into(), "/".into(), "-lra".into()]),
          attach_stderr: Some(true),
          attach_stdout: Some(true),
          ..Default::default()
        }),
        Some(&GenericNspQuery {
          namespace: Some("system".into()),
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "cargo create exec");
    let data = res.json::<CreateExecResults>().await.unwrap();
    let exec_id = data.id;
    let res = client
      .send_post(
        &format!("/exec/{exec_id}/cargo/start"),
        Some(&StartExecOptions::default()),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "exec start");
    let mut stream = res.into_stream();
    while (stream.next().await).is_some() {}
    let mut res = client
      .send_get(&format!("/exec/{exec_id}/cargo/inspect"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "exec inspect");
    let data = res.json::<ExecInspectResponse>().await.unwrap();
    assert_eq!(data.exit_code, Some(0), "Expect exit code to be 0");
  }
}
