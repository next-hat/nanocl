use futures::{stream::select_all, StreamExt, TryStreamExt};
use ntex::web;

use bollard_next::container::LogsOptions;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::{ProcessLogQuery, ProcessOutputLog};

use crate::{
  models::{ProcessDb, SystemState},
  utils,
};

/// Get logs of a single process instance by it's name or id
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{name}/logs",
  params(
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("since" = Option<i64>, Query, description = "Only logs returned since timestamp"),
    ("until" = Option<i64>, Query, description = "Only logs returned until timestamp"),
    ("timestamps" = Option<bool>, Query, description = "Add timestamps to every log line"),
    ("follow" = Option<bool>, Query, description = "Boolean to return a stream or not"),
    ("tail" = Option<String>, Query, description = "Only return the n last (integer) or all (\"all\") logs"),
  ),
  responses(
    (status = 200, description = "Process instances logs", content_type = "application/vdn.nanocl.raw-stream"),
  ),
))]
#[web::get("/processes/{name}/logs")]
async fn logs_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  log::debug!("process::logs_process: {name}");
  let options: LogsOptions<String> = qs.into_inner().into();
  let stream = state
    .inner
    .docker_api
    .logs(
      &name,
      Some(LogsOptions::<String> {
        stdout: true,
        stderr: true,
        ..options.clone()
      }),
    )
    .map(move |elem| match elem {
      Err(err) => Err(err),
      Ok(elem) => Ok(ProcessOutputLog {
        name: name.clone(),
        log: elem.into(),
      }),
    });
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

/// Get logs of processes of given kind and name (cargo, job, vm)
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
  ),
))]
#[web::get("/processes/{kind}/{name}/logs")]
async fn logs_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  let processes =
    ProcessDb::read_by_kind_key(&kind_key, None, &state.inner.pool).await?;
  let options: LogsOptions<String> = qs.into_inner().into();
  let futures = processes
    .into_iter()
    .filter(|process| !process.name.starts_with("tmp-"))
    .collect::<Vec<_>>()
    .into_iter()
    .map(|process| {
      state
        .inner
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
