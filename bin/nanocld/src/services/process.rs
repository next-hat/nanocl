use ntex::web;
use futures_util::stream::select_all;
use futures_util::{StreamExt, TryStreamExt};

use nanocl_error::http::HttpResult;

use bollard_next::container::LogsOptions;
use nanocl_stubs::{
  generic::{GenericNspQuery, GenericFilter},
  process::{ProcessLogQuery, ProcessOutputLog, ProccessQuery},
};

use crate::utils;
use crate::models::{DaemonState, Repository, ProcessDb};

/// List process (Vm, Job, Cargo)
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes",
  params(
    ("all" = bool, Query, description = "Return instances from all nodes"),
    ("last" = Option<isize>, Query, description = "Return this number of most recently created containers"),
    ("namespace" = Option<String>, Query, description = "Return instances from this namespace only"),
  ),
  responses(
    (status = 200, description = "List of instances", body = [Process]),
  ),
))]
#[web::get("/processes")]
pub(crate) async fn list_process(
  state: web::types::State<DaemonState>,
  _: web::types::Query<ProccessQuery>,
) -> HttpResult<web::HttpResponse> {
  let processes =
    ProcessDb::find(&GenericFilter::default(), &state.pool).await??;
  Ok(web::HttpResponse::Ok().json(&processes))
}

/// Get logs of a process
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{name}/logs",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace of the process"),
    ("since" = Option<i64>, Query, description = "Only logs returned since timestamp"),
    ("until" = Option<i64>, Query, description = "Only logs returned until timestamp"),
    ("timestamps" = Option<bool>, Query, description = "Add timestamps to every log line"),
    ("follow" = Option<bool>, Query, description = "Boolean to return a stream or not"),
    ("tail" = Option<String>, Query, description = "Only return the n last (integer) or all (\"all\") logs"),
  ),
  responses(
    (status = 200, description = "Process instances logs", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Process don't exist"),
  ),
))]
#[web::get("/processes/{kind}/{name}/logs")]
async fn logs_process(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = utils::process::parse_kind(&kind)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  let processes = ProcessDb::find_by_kind_key(&kind_key, &state.pool).await?;
  log::debug!("process::logs_process: {kind_key}");
  let options: LogsOptions<String> = qs.into_inner().into();
  let futures = processes
    .into_iter()
    .map(|process| {
      state
        .docker_api
        .logs(
          &process.data.id.unwrap_or_default(),
          Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            ..options.clone()
          }),
        )
        .map(move |elem| match elem {
          Err(err) => Err(err),
          Ok(elem) => Ok(ProcessOutputLog {
            name: process.name.clone(),
            log: elem.into(),
          }),
        })
    })
    .collect::<Vec<_>>();
  let stream = select_all(futures).into_stream();
  let stream = utils::stream::transform_stream::<
    ProcessOutputLog,
    ProcessOutputLog,
  >(stream);
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(stream),
  )
}

/// Start a process
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{name}/start",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the process belongs is needed"),
  ),
  responses(
    (status = 202, description = "Process started"),
    (status = 404, description = "Process does not exist"),
  ),
))]
#[web::post("/processes/{type}/{name}/start")]
pub(crate) async fn start_process(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = utils::process::parse_kind(&kind)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::process::start_by_kind(&kind, &kind_key, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Stop a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{name}/stop",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the process belongs is needed"),
  ),
  responses(
    (status = 202, description = "Cargo stopped"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/processes/{kind}/{name}/stop")]
pub(crate) async fn stop_process(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = utils::process::parse_kind(&kind)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::process::stop_by_kind(&kind, &kind_key, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_process);
  config.service(logs_process);
  config.service(start_process);
  config.service(stop_process);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use crate::utils::tests::*;

  use nanocl_stubs::process::{Process, ProccessQuery};

  #[ntex::test]
  async fn basic_list() {
    let client = gen_default_test_client().await;
    let mut res = client
      .send_get(
        "/processes",
        Some(&ProccessQuery {
          all: false,
          ..Default::default()
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let _ = res.json::<Vec<Process>>().await.unwrap();
  }
}
